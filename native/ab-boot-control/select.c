/* SPDX-License-Identifier: AGPL-3.0-only */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <inttypes.h>
#include <linux/fs.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/file.h>
#include <sys/ioctl.h>
#include <sys/stat.h>
#include <unistd.h>

enum { PAGE_BYTES = 4096, LEDGER_BYTES = 8 * 1024 * 1024, CHECKSUM = 28,
       GENERATION = 8, CONFIRMED = 16, PENDING = 17, VERIFIED = 18, TRIES = 19,
       NO_SLOT = 255, MAX_TRIES = 2 };
static const char magic[] = "RSHABCT1";

static uint64_t load_le(const uint8_t *bytes, size_t n)
{
    uint64_t value = 0;
    for (size_t i = 0; i < n; i++)
        value |= (uint64_t)bytes[i] << (8 * i);
    return value;
}

static void store_le(uint8_t *bytes, uint64_t value, size_t n)
{
    for (size_t i = 0; i < n; i++)
        bytes[i] = (uint8_t)(value >> (8 * i));
}

static uint32_t crc32(const uint8_t *data, size_t n)
{
    uint32_t crc = UINT32_MAX;
    for (size_t i = 0; i < n; i++) {
        crc ^= data[i];
        for (int bit = 0; bit < 8; bit++)
            crc = (crc >> 1) ^ ((crc & 1) ? UINT32_C(0xedb88320) : 0);
    }
    return crc ^ UINT32_MAX;
}

static bool validate(const uint8_t *record)
{
    if (memcmp(record, magic, sizeof(magic) - 1) ||
        load_le(record + GENERATION, 8) == 0 ||
        load_le(record + CHECKSUM, 4) != crc32(record, CHECKSUM))
        return false;
    for (size_t i = TRIES + 1; i < CHECKSUM; i++)
        if (record[i] != 0)
            return false;
    for (size_t i = CHECKSUM + 4; i < PAGE_BYTES; i++)
        if (record[i] != 0)
            return false;
    uint8_t confirmed = record[CONFIRMED], pending = record[PENDING];
    uint8_t verified = record[VERIFIED], tries = record[TRIES];
    if (confirmed > 1 || verified > 3 || !(verified & (1 << confirmed)))
        return false;
    if (pending == NO_SLOT)
        return tries == 0;
    return pending <= 1 && pending != confirmed && (verified & (1 << pending)) && tries <= MAX_TRIES;
}

static int load_record(int fd, uint8_t pages[2][PAGE_BYTES])
{
    bool valid[2];
    uint64_t generation[2];
    for (int i = 0; i < 2; i++) {
        if (pread(fd, pages[i], PAGE_BYTES, i * (LEDGER_BYTES / 2)) != PAGE_BYTES)
            return -1;
        valid[i] = validate(pages[i]);
        generation[i] = valid[i] ? load_le(pages[i] + GENERATION, 8) : 0;
    }
    if (!valid[0] && !valid[1])
        return -1;
    if (valid[0] && valid[1] && generation[0] == generation[1] &&
        memcmp(pages[0], pages[1], PAGE_BYTES) != 0)
        return -1;
    return generation[1] > generation[0] ? 1 : 0;
}

static bool save_record(int fd, uint8_t *record, int current)
{
    uint8_t receipt[PAGE_BYTES];
    uint64_t generation = load_le(record + GENERATION, 8);
    if (generation == UINT64_MAX)
        return false;
    store_le(record + GENERATION, generation + 1, 8);
    store_le(record + CHECKSUM, crc32(record, CHECKSUM), 4);
    off_t destination = (1 - current) * (LEDGER_BYTES / 2);
    // Retain the previous record while updating its peer.
    return pwrite(fd, record, PAGE_BYTES, destination) == PAGE_BYTES &&
           fsync(fd) == 0 &&
           pread(fd, receipt, PAGE_BYTES, destination) == PAGE_BYTES &&
           memcmp(receipt, record, PAGE_BYTES) == 0;
}

static bool select_slot(int fd, uint8_t *record, int current, uint8_t *slot)
{
    *slot = record[CONFIRMED];
    if (record[PENDING] == NO_SLOT)
        return true;
    if (record[TRIES] > 0) {
        record[TRIES]--;
        *slot = record[PENDING];
    } else {
        record[VERIFIED] &= (uint8_t)~(1 << record[PENDING]);
        record[PENDING] = NO_SLOT;
    }
    return save_record(fd, record, current);
}

static bool parse_session(char **argv, uint8_t *slot, uint64_t *generation)
{
    if (strlen(argv[3]) != 1 || (argv[3][0] != 'A' && argv[3][0] != 'B') ||
        !*argv[4] || strspn(argv[4], "0123456789") != strlen(argv[4]))
        return false;
    *slot = (uint8_t)(argv[3][0] - 'A');
    errno = 0;
    *generation = strtoull(argv[4], NULL, 10);
    return !errno && *generation > 0;
}

static bool update_session(int fd, uint8_t *record, int current, char **argv, bool rollback, bool check)
{
    uint8_t running;
    uint64_t generation;
    if (!parse_session(argv, &running, &generation) || generation != load_le(record + GENERATION, 8))
        return false;
    if (check)
        return running == record[PENDING] && record[TRIES] < MAX_TRIES;
    if (rollback) {
        uint8_t backup = 1 - running;
        if (running != record[CONFIRMED] || record[PENDING] != NO_SLOT ||
            !(record[VERIFIED] & (1 << backup)))
            return false;
        record[PENDING] = backup;
        record[TRIES] = MAX_TRIES;
    } else {
        if (running != record[PENDING] || record[TRIES] == MAX_TRIES)
            return false;
        record[CONFIRMED] = running;
        record[PENDING] = NO_SLOT;
        record[TRIES] = 0;
    }
    return save_record(fd, record, current);
}

int main(int argc, char **argv)
{
    if (argc < 3)
        return 2;
    const char *command = argv[1];
    size_t length = strlen(command);
    bool image = length > 6 && !strcmp(command + length - 6, "-image");
    if (image)
        length -= 6;
    bool simple = length == 6 && !memcmp(command, "select", 6);
    bool session = length == 14 && !memcmp(command, "select-session", 14);
    bool confirm = length == 7 && !memcmp(command, "confirm", 7);
    bool rollback = length == 8 && !memcmp(command, "rollback", 8);
    bool check = length == 11 && !memcmp(command, "check-trial", 11);
    if ((!simple && !session && !confirm && !rollback && !check) ||
        argc != (confirm || rollback || check ? 5 : 3))
        return 2;
    int fd = open(argv[2], O_RDWR | O_CLOEXEC | O_NOFOLLOW);
    struct stat st;
    uint64_t bytes = 0;
    bool ok = fd >= 0 && fstat(fd, &st) == 0;
    if (ok && image) {
        ok = S_ISREG(st.st_mode) && st.st_nlink == 1;
        bytes = (uint64_t)st.st_size;
    } else if (ok) {
        ok = S_ISBLK(st.st_mode) && ioctl(fd, BLKGETSIZE64, &bytes) == 0;
    }
    uint8_t pages[2][PAGE_BYTES], slot = NO_SLOT;
    int current = -1;
    if (ok && bytes == LEDGER_BYTES && flock(fd, LOCK_EX | LOCK_NB) == 0)
        current = load_record(fd, pages);
    ok = current >= 0;
    if (ok && (simple || session))
        ok = select_slot(fd, pages[current], current, &slot);
    else if (ok)
        ok = update_session(fd, pages[current], current, argv, rollback, check);
    if (fd >= 0)
        close(fd);
    if (!ok) {
        fputs("A/B boot control record unavailable or operation rejected\n", stderr);
        return 1;
    }
    if (session)
        printf("%c %" PRIu64 " %s\n", 'A' + slot,
               load_le(pages[current] + GENERATION, 8),
               slot == pages[current][CONFIRMED] ? "stable" : "trial");
    else if (confirm || rollback)
        printf("%c %" PRIu64 "\n", argv[3][0], load_le(pages[current] + GENERATION, 8));
    else if (simple)
        puts(slot == 0 ? "A" : "B");
    return 0;
}
