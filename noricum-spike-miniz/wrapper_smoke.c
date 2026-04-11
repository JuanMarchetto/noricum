/* Pure-C smoke test for wrapper.c. Compile and run BEFORE writing any Rust.
 *
 *   gcc wrapper_smoke.c wrapper.c miniz.c miniz_zip.c -o wrapper_smoke
 *   ./wrapper_smoke
 *
 * Must return 0. If this fails, wrapper.c has a bug and Rust is not the problem.
 */

#include "wrapper.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int fail(const char* msg) {
    fprintf(stderr, "FAIL: %s\n", msg);
    return 1;
}

int main(int argc, char** argv) {
    const char* zip_path = (argc > 1) ? argv[1] : "fixtures/zip_corpus/hello.zip";

    zip_handle_t h = wr_reader_open(zip_path);
    if (!h) return fail("wr_reader_open returned NULL");

    int num = wr_reader_get_num_files(h);
    if (num != 1) {
        wr_reader_close(h);
        fprintf(stderr, "FAIL: expected 1 file, got %d\n", num);
        return 1;
    }

    int idx = wr_reader_locate(h, "hello.txt");
    if (idx < 0) {
        wr_reader_close(h);
        return fail("wr_reader_locate could not find hello.txt");
    }

    char name[256];
    size_t size = 0;
    unsigned int crc32 = 0;
    if (wr_reader_stat(h, idx, name, sizeof(name), &size, &crc32) != 0) {
        wr_reader_close(h);
        return fail("wr_reader_stat returned non-zero");
    }

    if (strcmp(name, "hello.txt") != 0) {
        wr_reader_close(h);
        fprintf(stderr, "FAIL: name mismatch, got '%s'\n", name);
        return 1;
    }

    if (size != 6) {
        wr_reader_close(h);
        fprintf(stderr, "FAIL: size mismatch, got %zu (expected 6)\n", size);
        return 1;
    }

    char buf[16] = {0};
    if (wr_reader_extract(h, idx, buf, sizeof(buf)) != 0) {
        wr_reader_close(h);
        return fail("wr_reader_extract returned non-zero");
    }

    if (memcmp(buf, "hello\n", 6) != 0) {
        wr_reader_close(h);
        fprintf(stderr, "FAIL: content mismatch, got '%.*s'\n", 6, buf);
        return 1;
    }

    wr_reader_close(h);

    printf("smoke: OK  (name='%s' size=%zu crc32=0x%08x)\n", name, size, crc32);
    return 0;
}
