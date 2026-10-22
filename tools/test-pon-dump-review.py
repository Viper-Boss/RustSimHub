#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Execute the actual diagnostic init function with mock kernel dependencies.

This tests error-path accounting, not kernel ABI, regmap lifetime or hardware.
Use --original FILE to additionally demonstrate the submitted snippet's leak.
"""
import argparse
import json
import pathlib
import shutil
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
HEADER = r'''
#ifndef PON_MOCK_H
#define PON_MOCK_H
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <errno.h>
typedef uint16_t u16;
typedef uint32_t u32;
#define __init
#define __exit
#define ARRAY_SIZE(a) (sizeof(a)/sizeof((a)[0]))
#define pr_info(...) ((void)0)
#define module_init(f)
#define module_exit(f)
#define MODULE_DESCRIPTION(s)
#define MODULE_LICENSE(s)
struct device_node { int placeholder; };
struct device { struct device *parent; };
struct platform_device { struct device dev; };
struct regmap { int placeholder; };
static const char *scenario;
static int node_refs, device_refs, reads;
static struct device_node node;
static struct device parent;
static struct platform_device platform = { .dev = { .parent = &parent } };
static struct regmap map;
static struct device_node *of_find_compatible_node(void *a, void *b, const char *c)
{ (void)a; (void)b; (void)c; if (!strcmp(scenario,"no_node")) return NULL; node_refs++; return &node; }
static void of_node_put(struct device_node *n) { (void)n; node_refs--; }
static int of_device_is_available(struct device_node *n)
{ (void)n; return strcmp(scenario,"disabled") != 0; }
static int of_property_read_u32(struct device_node *n, const char *key, u32 *v)
{ (void)n; (void)key; if (!strcmp(scenario,"reg_error")) return -EINVAL; *v=!strcmp(scenario,"bad_base")?0xffffU:0x800U; return 0; }
static struct platform_device *of_find_device_by_node(struct device_node *n)
{ (void)n; if (!strcmp(scenario,"no_device")) return NULL; device_refs++; return &platform; }
static void put_device(struct device *d) { (void)d; device_refs--; }
static struct regmap *dev_get_regmap(struct device *d, const char *name)
{ (void)name; return d && strcmp(scenario,"no_map") ? &map : NULL; }
static int regmap_read(struct regmap *m, unsigned int reg, unsigned int *v)
{ (void)m; (void)reg; reads++; *v=0; return !strcmp(scenario,"all_read_errors") || (!strcmp(scenario,"one_read_error") && reads==1) ? -EIO : 0; }
#endif
'''
MAIN = r'''
#include "subject.c"
int main(int argc, char **argv) {
 if (argc!=2) return 99;
 scenario=argv[1];
 if (!strcmp(scenario,"no_parent")) platform.dev.parent=NULL;
 int result=pon_dump_init();
 if (!result) pon_dump_exit();
 printf("{\"result\":%d,\"node_refs\":%d,\"device_refs\":%d,\"reads\":%d}\n",result,node_refs,device_refs,reads);
 return 0;
}
'''


def build(source, directory):
    directory.mkdir()
    headers = directory / 'linux'
    headers.mkdir()
    (directory / 'mock.h').write_text(HEADER)
    for name in ['module.h', 'of.h', 'of_platform.h', 'platform_device.h', 'regmap.h']:
        (headers / name).write_text('#include "mock.h"\n')
    shutil.copyfile(source, directory / 'subject.c')
    (directory / 'main.c').write_text(MAIN)
    binary = directory / 'check'
    # Unused mocked helpers vary by source version; real ABI is checked separately.
    subprocess.run(['cc', '-Wall', '-Wextra', '-Werror', '-Wno-unused-function',
                    '-I', str(directory), str(directory / 'main.c'), '-o', str(binary)], check=True)
    return binary


def run(binary, scenario):
    return json.loads(subprocess.check_output([str(binary), scenario], text=True))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--original', type=pathlib.Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='pon-dump-refs-') as name:
        directory = pathlib.Path(name)
        fixed = build(ROOT / 'tools/diagnostics/pon-dump/pon_dump.c', directory / 'reviewed')
        cases = [('reg_error', -22, 0), ('no_node', -19, 0), ('disabled', -19, 0),
                 ('bad_base', -34, 0), ('no_device', -19, 0), ('no_parent', -19, 0),
                 ('no_map', -19, 0), ('success', 0, 9),
                 ('one_read_error', -5, 9), ('all_read_errors', -5, 9)]
        for scenario, result, reads in cases:
            actual = run(fixed, scenario)
            assert actual == {'result': result, 'node_refs': 0, 'device_refs': 0, 'reads': reads}, (scenario, actual)
        if args.original:
            original = build(args.original.resolve(), directory / 'original')
            leaked = run(original, 'reg_error')
            assert leaked == {'result': -22, 'node_refs': 0, 'device_refs': 1, 'reads': 0}, leaked
            failed_reads = run(original, 'all_read_errors')
            assert failed_reads['result'] == 0 and failed_reads['reads'] == 10, failed_reads
            print('Original snippet reproduced: failed reg read leaks device reference; all register read errors still return success.')
        print('PASS: 10 actual-init control-flow cases; mock references balanced, failed reads reported. No kernel ABI or hardware behavior simulated as proof.')


if __name__ == '__main__':
    main()
