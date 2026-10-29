# SPDX-License-Identifier: AGPL-3.0-only
import copy
import json
import os
import pathlib
import tempfile
import time
import unittest
from unittest.mock import patch
import measure
import compare

class MeasureTests(unittest.TestCase):
    def test_default_is_plan_and_rejects_arbitrary_profile_or_budget(self):
        self.assertFalse(measure.plan()['runs_processes'])
        for profile, seconds, warmup, interval in [('remote',1,0,.1),('call',float('nan'),0,.1),('call',21,0,.1),('call',1,0,0)]:
            with self.assertRaises(ValueError):
                measure.measure(profile, seconds, warmup, interval)

    def test_identity_loss_and_nonpositive_or_decreased_counter_are_gaps(self):
        self.assertIsNone(measure.delta({'ticks':10},{'ticks':9},1))
        self.assertIsNone(measure.delta({'ticks':1},{'ticks':2},0))
        self.assertIsNone(measure.delta(None,{'ticks':2},1))
        self.assertEqual(measure.delta({'ticks':0},{'ticks':os.sysconf('SC_CLK_TCK')},1),100)
        with patch.object(measure,'read_stat',return_value={'start':2}):
            self.assertIsNone(measure.sample_tree(100,1))
        with patch.object(measure,'read_stat',return_value=None):
            self.assertIsNone(measure.sample_tree(100,1))

    def test_real_workload_finishes_and_cancel_cleans_only_own_children(self):
        result=measure.measure('sms',seconds=.5,warmup=0,interval=.05)
        self.assertEqual(result['outcome'],'completed')
        self.assertTrue(result['cleanup_verified'])
        self.assertGreater(result['completed_iterations'],0)
        self.assertGreater(result['sample_count'],0)
        self.assertFalse(result['device_performance_verified'])
        started=time.monotonic()
        result=measure.measure('call',seconds=2,warmup=0,interval=.05,cancel=lambda:time.monotonic()-started>.2)
        self.assertEqual(result['outcome'],'cancelled')
        self.assertTrue(result['cleanup_verified'])
        self.assertLess(time.monotonic()-started,3)

    def test_private_report_never_overwrites_or_follows_symlink(self):
        with tempfile.TemporaryDirectory() as tmp:
            report=pathlib.Path(tmp)/'report.json'
            measure.write_report(report,{'synthetic':True})
            self.assertEqual(report.stat().st_mode&0o777,0o600)
            with self.assertRaises(FileExistsError): measure.write_report(report,{'overwrite':True})
            alias=pathlib.Path(tmp)/'alias';alias.symlink_to(report)
            with self.assertRaises(FileExistsError): measure.write_report(alias,{})
            self.assertEqual(json.loads(report.read_text()),{'synthetic':True})

class CompareTests(unittest.TestCase):
    def report(self):
        return {'schema':1,'kind':'synthetic-functional','scope':measure.SCOPE,'profile':'sms',
                'source_sha':'a'*40,'source_dirty':False,'runner_sha256':'b'*64,'workload_sha256':'c'*64,
                'outcome':'completed','cleanup_verified':True,'device_performance_verified':False,'budget_approved':False,
                'environment':{'architecture':'synthetic'},'completed_iterations':2,'sample_count':11,
                'samples':[{'elapsed_s':n/2,'cpu_percent':10,'rss_bytes':2048,'processes':1} for n in range(11)],
                'mean_cpu_percent':999,'peak_rss_bytes':999}
    def run_compare(self,a,b=None):
        with tempfile.TemporaryDirectory() as tmp:
            old=pathlib.Path(tmp)/'old.json';new=pathlib.Path(tmp)/'new.json'
            old.write_text(json.dumps(a));new.write_text(json.dumps(b or a))
            return compare.compare(old,new,'a'*40,'a'*40)

    def test_recomputes_units_and_never_claims_device_or_budget_acceptance(self):
        r=self.run_compare(self.report())
        self.assertEqual(r['metrics']['mean_cpu_percent']['old'],10)
        self.assertEqual(r['metrics']['peak_rss_bytes']['old'],2048)
        self.assertEqual(r['budget_verdict'],'not_assessed')
        self.assertFalse(r['device_performance_verified'])
        self.assertTrue(r['same_function_review_required'])

    def test_incomplete_old_sha_unknown_units_and_short_samples_are_rejected(self):
        for key,value in [('schema',2),('source_sha','d'*40),('source_dirty',True),('outcome','cancelled'),
                          ('cleanup_verified',False),('sample_count',0),('completed_iterations',0),('scope','MiB instead of bytes')]:
            a=self.report();a[key]=value
            with self.subTest(key=key),self.assertRaises(ValueError):self.run_compare(a)
        for key,value in [('cpu_percent',float('nan')),('cpu_percent',-1),('rss_bytes','2 MiB'),('elapsed_s',-1)]:
            a=self.report();a['samples'][3][key]=value
            with self.subTest(key=key),self.assertRaises(ValueError):self.run_compare(a)
        a=self.report()
        for s in a['samples']:s['elapsed_s']/=10
        with self.assertRaises(ValueError):self.run_compare(a)
        a=self.report();b=copy.deepcopy(a);b['environment']={'architecture':'other'}
        with self.assertRaises(ValueError):self.run_compare(a,b)

    def test_links_and_oversize_input_are_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            p=pathlib.Path(tmp)/'report';p.write_text(json.dumps(self.report()))
            link=pathlib.Path(tmp)/'link';link.symlink_to(p)
            with self.assertRaises(OSError):compare.load(link,'a'*40)
            p.write_bytes(b' '* (measure.MAX_BYTES+1))
            with self.assertRaises(ValueError):compare.load(p,'a'*40)

if __name__=='__main__':unittest.main()
