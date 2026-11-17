# SPDX-License-Identifier: AGPL-3.0-only
"""Windows owned Job Object. Children join before loading any test module."""
import ctypes
from ctypes import wintypes as w
import os
import uuid


class Basic(ctypes.Structure):
    _fields_ = [('process_time', ctypes.c_longlong), ('job_time', ctypes.c_longlong),
                ('flags', w.DWORD), ('min_working_set', ctypes.c_size_t),
                ('max_working_set', ctypes.c_size_t), ('process_limit', w.DWORD),
                ('affinity', ctypes.c_size_t), ('priority', w.DWORD), ('scheduling', w.DWORD)]


class IO(ctypes.Structure):
    _fields_ = [(name, ctypes.c_ulonglong) for name in
                ('read_operations', 'write_operations', 'other_operations', 'read_bytes', 'write_bytes', 'other_bytes')]


class Extended(ctypes.Structure):
    _fields_ = [('basic', Basic), ('io', IO), ('process_memory', ctypes.c_size_t),
                ('job_memory', ctypes.c_size_t), ('peak_process_memory', ctypes.c_size_t),
                ('peak_job_memory', ctypes.c_size_t)]


class Accounting(ctypes.Structure):
    _fields_ = [(name, ctypes.c_longlong) for name in ('user', 'kernel', 'period_user', 'period_kernel')] + [
        (name, w.DWORD) for name in ('faults', 'total', 'active', 'terminated')]


class ProcessList(ctypes.Structure):
    _fields_ = [('assigned', w.DWORD), ('count', w.DWORD), ('pids', ctypes.c_size_t * 32)]


class Memory(ctypes.Structure):
    _fields_ = [('size', w.DWORD), ('faults', w.DWORD)] + [(name, ctypes.c_size_t) for name in (
        'peak_working_set', 'working_set', 'peak_paged_pool', 'paged_pool',
        'peak_nonpaged_pool', 'nonpaged_pool', 'pagefile', 'peak_pagefile', 'private')]


def api():
    if os.name != 'nt':
        raise OSError('Windows Job Objects unavailable')
    k = ctypes.WinDLL('kernel32', use_last_error=True)
    signatures = {
        'CreateJobObjectW': ([ctypes.c_void_p, w.LPCWSTR], w.HANDLE),
        'OpenJobObjectW': ([w.DWORD, w.BOOL, w.LPCWSTR], w.HANDLE),
        'SetInformationJobObject': ([w.HANDLE, ctypes.c_int, ctypes.c_void_p, w.DWORD], w.BOOL),
        'QueryInformationJobObject': ([w.HANDLE, ctypes.c_int, ctypes.c_void_p, w.DWORD, ctypes.c_void_p], w.BOOL),
        'AssignProcessToJobObject': ([w.HANDLE, w.HANDLE], w.BOOL),
        'TerminateJobObject': ([w.HANDLE, w.UINT], w.BOOL),
        'GetCurrentProcess': ([], w.HANDLE), 'CloseHandle': ([w.HANDLE], w.BOOL),
        'OpenProcess': ([w.DWORD, w.BOOL, w.DWORD], w.HANDLE),
        'IsProcessInJob': ([w.HANDLE, w.HANDLE, ctypes.POINTER(w.BOOL)], w.BOOL),
        'K32GetProcessMemoryInfo': ([w.HANDLE, ctypes.POINTER(Memory), w.DWORD], w.BOOL),
    }
    for name, (args, result) in signatures.items():
        getattr(k, name).argtypes, getattr(k, name).restype = args, result
    return k


def need(value):
    if not value:
        raise ctypes.WinError(ctypes.get_last_error())
    return value


class Job:
    def __init__(self):
        self.k = api()
        self.name = 'Local\\RustSimHubOfflineB-' + uuid.uuid4().hex
        self.handle = need(self.k.CreateJobObjectW(None, self.name))
        limits = Extended()
        # No breakaway permission: descendants remain in this owned job.
        limits.basic.flags = 0x2000 | 0x8 | 0x200  # KILL_ON_JOB_CLOSE, ACTIVE_PROCESS, JOB_MEMORY
        limits.basic.process_limit = 32
        limits.job_memory = 1024 * 1024 * 1024
        try:
            need(self.k.SetInformationJobObject(self.handle, 9, ctypes.byref(limits), ctypes.sizeof(limits)))
        except BaseException:
            self.close()
            raise

    def active(self):
        info = Accounting()
        need(self.k.QueryInformationJobObject(self.handle, 1, ctypes.byref(info), ctypes.sizeof(info), None))
        return info.active

    def resources(self):
        """Only owned job members, checked again after opening each PID handle.

        CPU is cumulative job-member CPU time. RSS is the current summed working
        set of readable members, not committed bytes or an OS peak-RSS counter.
        Missing measurements are gaps, never an assumed zero.
        """
        accounting, limits, members = Accounting(), Extended(), ProcessList()
        for kind, value in ((1, accounting), (9, limits), (3, members)):
            need(self.k.QueryInformationJobObject(self.handle, kind, ctypes.byref(value), ctypes.sizeof(value), None))
        if members.count > 32:
            raise OSError('owned job exceeded fixed process-list budget')
        rss, gaps, measured = 0, 0, 0
        for pid in members.pids[:members.count]:
            process = self.k.OpenProcess(0x0400 | 0x0010, False, pid)  # query info / VM counters
            if not process:
                gaps += 1
                continue
            try:
                belongs = w.BOOL()
                if not self.k.IsProcessInJob(process, self.handle, ctypes.byref(belongs)) or not belongs.value:
                    gaps += 1  # exited/reused PID: do not inspect an unrelated process
                    continue
                memory = Memory()
                memory.size = ctypes.sizeof(memory)
                if not self.k.K32GetProcessMemoryInfo(process, ctypes.byref(memory), memory.size):
                    gaps += 1
                    continue
                measured += 1
                rss += memory.working_set
            finally:
                self.k.CloseHandle(process)
        return {'cpu_seconds': (accounting.user + accounting.kernel) / 10_000_000,
                'rss_bytes': rss if measured else None, 'rss_gaps': gaps,
                'members_observed': members.count, 'members_measured': measured,
                'peak_job_committed_bytes': limits.peak_job_memory}

    def stop(self):
        need(self.k.TerminateJobObject(self.handle, 125))

    def close(self):
        if self.handle:
            self.k.CloseHandle(self.handle)
            self.handle = None


def join(name):
    prefix = 'Local\\RustSimHubOfflineB-'
    if not name.startswith(prefix) or len(name) != len(prefix) + 32:
        raise ValueError('invalid owned job name')
    int(name[len(prefix):], 16)
    k = api()
    handle = need(k.OpenJobObjectW(1, False, name))  # JOB_OBJECT_ASSIGN_PROCESS
    try:
        need(k.AssignProcessToJobObject(handle, k.GetCurrentProcess()))
    finally:
        k.CloseHandle(handle)
