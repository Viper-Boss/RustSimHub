/* SPDX-License-Identifier: AGPL-3.0-only */
#define _GNU_SOURCE
#include <fcntl.h>
#include <linux/fs.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
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

static bool select_slot(int fd, uint8_t *slot)
{
    uint8_t pages[2][PAGE_BYTES], receipt[PAGE_BYTES];
    bool valid[2];
    uint64_t generation[2];
    for (int i = 0; i < 2; i++) {
        if (pread(fd, pages[i], PAGE_BYTES, i * (LEDGER_BYTES / 2)) != PAGE_BYTES)
            return false;
        valid[i] = validate(pages[i]);
        generation[i] = valid[i] ? load_le(pages[i] + GENERATION, 8) : 0;
    }
    if (!valid[0] && !valid[1])
        return false;
    if (valid[0] && valid[1] && generation[0] == generation[1] &&
        memcmp(pages[0], pages[1], PAGE_BYTES) != 0)
        return false;
    int current = generation[1] > generation[0] ? 1 : 0;
    uint8_t *record = pages[current];
    *slot = record[CONFIRMED];
    if (record[PENDING] == NO_SLOT)
        return true;
    if (generation[current] == UINT64_MAX)
        return false;

    if (record[TRIES] > 0) {
        record[TRIES]--;
        *slot = record[PENDING];
    } else {
        record[VERIFIED] &= (uint8_t)~(1 << record[PENDING]);
        record[PENDING] = NO_SLOT;
    }
    store_le(record + GENERATION, generation[current] + 1, 8);
    store_le(record + CHECKSUM, crc32(record, CHECKSUM), 4);
    off_t destination = (1 - current) * (LEDGER_BYTES / 2);
    // Preserve the previous record while replacing its peer. A candidate is
    // returned only after the write is durable and read back verbatim.
    return pwrite(fd, record, PAGE_BYTES, destination) == PAGE_BYTES &&
           fsync(fd) == 0 &&
           pread(fd, receipt, PAGE_BYTES, destination) == PAGE_BYTES &&
           memcmp(receipt, record, PAGE_BYTES) == 0;
}

int main(int argc, char **argv)
{
    if (argc != 3 || (strcmp(argv[1], "select") && strcmp(argv[1], "select-image"))) {
        fputs("usage: rustsimhub-ab-select select|select-image <boot control>\n", stderr);
        return 2;
    }
    bool image = !strcmp(argv[1], "select-image");
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
    uint8_t slot;
    ok = ok && bytes == LEDGER_BYTES && flock(fd, LOCK_EX | LOCK_NB) == 0 && select_slot(fd, &slot);
    if (fd >= 0)
        close(fd);
    if (!ok) {
        fputs("A/B boot control record unavailable or update failed\n", stderr);
        return 1;
    }
    puts(slot == 0 ? "A" : "B");
    return 0;
}
