// SPDX-License-Identifier: AGPL-3.0-only
// Synthetic executable for the sealed metadata process boundary. No devices,
// data paths, credentials or network operations are implemented here.
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#ifndef TEST_WAIT
#define TEST_WAIT 0
#endif
#ifndef TEST_FLOOD
#define TEST_FLOOD 0
#endif

int main(int argc, char **argv) {
    if (argc != 2 || strcmp(argv[1], "--build-info") != 0) return 2;
    if (TEST_WAIT) {
        sleep(20);
        return 3;
    }
    if (TEST_FLOOD) {
        for (;;) puts("synthetic-output-budget-check");
    }
    printf("{\"uid\":%lu,\"gid\":%lu,\"groups\":%d,\"clean_env\":%s}\n",
           (unsigned long)geteuid(), (unsigned long)getegid(), getgroups(0, NULL),
           getenv("RUSTSIMHUB_SYNTHETIC_PARENT_SECRET") == NULL ? "true" : "false");
    return 0;
}
