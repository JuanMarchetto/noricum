#ifndef NORICUM_SPIKE_WRAPPER_H
#define NORICUM_SPIKE_WRAPPER_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque handle. Caller never peeks inside. */
typedef void* zip_handle_t;

/* Reader API. All functions return 0 on success, negative on failure, or a
 * positive index for locate. A NULL handle return from wr_reader_open means
 * open failed (e.g., file missing or not a valid zip). */
zip_handle_t wr_reader_open(const char* filename);
int          wr_reader_get_num_files(zip_handle_t h);
int          wr_reader_locate(zip_handle_t h, const char* name);
int          wr_reader_extract(zip_handle_t h, int file_index, void* buf, size_t buf_size);
int          wr_reader_stat(zip_handle_t h,
                            int file_index,
                            char* name_out,
                            size_t name_cap,
                            size_t* size_out,
                            unsigned int* crc32_out);
void         wr_reader_close(zip_handle_t h);

/* Writer API. All functions return 0 on success, negative on failure. */
zip_handle_t wr_writer_open(const char* filename);
int          wr_writer_add(zip_handle_t h, const char* name, const void* buf, size_t buf_size);
int          wr_writer_finalize(zip_handle_t h);
void         wr_writer_close(zip_handle_t h);

#ifdef __cplusplus
}
#endif

#endif /* NORICUM_SPIKE_WRAPPER_H */
