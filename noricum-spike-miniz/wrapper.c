#include "wrapper.h"
#include "miniz.h"
#include "miniz_zip.h"

#include <stdlib.h>
#include <string.h>

typedef struct {
    mz_zip_archive zip;
    int is_writer;
} wrapper_ctx;

zip_handle_t wr_reader_open(const char* filename) {
    if (!filename) return NULL;
    wrapper_ctx* ctx = (wrapper_ctx*)calloc(1, sizeof(wrapper_ctx));
    if (!ctx) return NULL;
    if (!mz_zip_reader_init_file(&ctx->zip, filename, 0)) {
        free(ctx);
        return NULL;
    }
    ctx->is_writer = 0;
    return (zip_handle_t)ctx;
}

int wr_reader_get_num_files(zip_handle_t h) {
    if (!h) return -1;
    wrapper_ctx* ctx = (wrapper_ctx*)h;
    return (int)mz_zip_reader_get_num_files(&ctx->zip);
}

int wr_reader_locate(zip_handle_t h, const char* name) {
    if (!h || !name) return -1;
    wrapper_ctx* ctx = (wrapper_ctx*)h;
    return mz_zip_reader_locate_file(&ctx->zip, name, NULL, 0);
}

int wr_reader_extract(zip_handle_t h, int file_index, void* buf, size_t buf_size) {
    if (!h || !buf) return -1;
    wrapper_ctx* ctx = (wrapper_ctx*)h;
    if (file_index < 0) return -1;
    return mz_zip_reader_extract_to_mem(&ctx->zip, (mz_uint)file_index, buf, buf_size, 0) ? 0 : -1;
}

int wr_reader_stat(zip_handle_t h,
                   int file_index,
                   char* name_out,
                   size_t name_cap,
                   size_t* size_out,
                   unsigned int* crc32_out) {
    if (!h || !name_out || !size_out || !crc32_out) return -1;
    if (file_index < 0) return -1;
    wrapper_ctx* ctx = (wrapper_ctx*)h;
    mz_zip_archive_file_stat stat;
    if (!mz_zip_reader_file_stat(&ctx->zip, (mz_uint)file_index, &stat)) return -1;
    size_t name_len = strlen(stat.m_filename);
    if (name_len >= name_cap) name_len = name_cap - 1;
    memcpy(name_out, stat.m_filename, name_len);
    name_out[name_len] = '\0';
    *size_out = (size_t)stat.m_uncomp_size;
    *crc32_out = (unsigned int)stat.m_crc32;
    return 0;
}

void wr_reader_close(zip_handle_t h) {
    if (!h) return;
    wrapper_ctx* ctx = (wrapper_ctx*)h;
    mz_zip_reader_end(&ctx->zip);
    free(ctx);
}

zip_handle_t wr_writer_open(const char* filename) {
    if (!filename) return NULL;
    wrapper_ctx* ctx = (wrapper_ctx*)calloc(1, sizeof(wrapper_ctx));
    if (!ctx) return NULL;
    if (!mz_zip_writer_init_file(&ctx->zip, filename, 0)) {
        free(ctx);
        return NULL;
    }
    ctx->is_writer = 1;
    return (zip_handle_t)ctx;
}

int wr_writer_add(zip_handle_t h, const char* name, const void* buf, size_t buf_size) {
    if (!h || !name || !buf) return -1;
    wrapper_ctx* ctx = (wrapper_ctx*)h;
    return mz_zip_writer_add_mem(&ctx->zip, name, buf, buf_size, MZ_DEFAULT_LEVEL) ? 0 : -1;
}

int wr_writer_finalize(zip_handle_t h) {
    if (!h) return -1;
    wrapper_ctx* ctx = (wrapper_ctx*)h;
    return mz_zip_writer_finalize_archive(&ctx->zip) ? 0 : -1;
}

void wr_writer_close(zip_handle_t h) {
    if (!h) return;
    wrapper_ctx* ctx = (wrapper_ctx*)h;
    mz_zip_writer_end(&ctx->zip);
    free(ctx);
}
