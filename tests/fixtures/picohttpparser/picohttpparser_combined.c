/*
 * picohttpparser Combined — self-contained HTTP request/response/chunked parser + tests.
 * For Noricum diff testing and LLM migration benchmarking.
 *
 * Based on the MIT-licensed picohttpparser by Kazuho Oku et al.
 * (https://github.com/h2o/picohttpparser).
 *
 * Inlines picohttpparser (parser) + picotest (TAP test framework) + test.c
 * into a single file. The mmap-based guard page is replaced with a plain
 * malloc buffer so the file is fully portable.
 *
 * Compile: gcc -std=gnu11 -Wall -o test picohttpparser_combined.c
 */
#include <assert.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>

/* ================================================================
 * SECTION 1: picotest — minimal TAP test framework (inlined)
 * ================================================================ */

struct test_t {
    int num_tests;
    int failed;
};

static struct test_t main_tests, *cur_tests = &main_tests;
static int test_level = 0;

static void indent(void) {
    for (int i = 0; i != test_level; ++i)
        printf("    ");
}

__attribute__((format(printf, 1, 2)))
static void note(const char *fmt, ...) {
    va_list arg;
    indent();
    printf("# ");
    va_start(arg, fmt);
    vprintf(fmt, arg);
    va_end(arg);
    printf("\n");
}

__attribute__((format(printf, 2, 3)))
static void _ok(int cond, const char *fmt, ...) {
    va_list arg;
    if (!cond) cur_tests->failed = 1;
    indent();
    printf("%s %d - ", cond ? "ok" : "not ok", ++cur_tests->num_tests);
    va_start(arg, fmt);
    vprintf(fmt, arg);
    va_end(arg);
    printf("\n");
}

#define ok(cond) _ok(cond, "check")

static int done_testing(void) {
    indent();
    printf("1..%d\n", cur_tests->num_tests);
    return cur_tests->failed;
}

static void subtest(const char *name, void (*cb)(void)) {
    struct test_t test = {0}, *parent_tests;
    parent_tests = cur_tests;
    cur_tests = &test;
    ++test_level;
    note("Subtest: %s", name);
    cb();
    done_testing();
    --test_level;
    cur_tests = parent_tests;
    if (test.failed) cur_tests->failed = 1;
    _ok(!test.failed, "%s", name);
}

/* ================================================================
 * SECTION 2: picohttpparser — HTTP parser (inlined from picohttpparser.h/c)
 * ================================================================ */

/* --- picohttpparser.h structures --- */

struct phr_header {
    const char *name;
    size_t name_len;
    const char *value;
    size_t value_len;
};

struct phr_chunked_decoder {
    size_t bytes_left_in_chunk;
    char consume_trailer;
    char _hex_count;
    char _state;
    uint64_t _total_read;
    uint64_t _total_overhead;
};

/* --- picohttpparser.c implementation --- */

#if __GNUC__ >= 3
#define likely(x) __builtin_expect(!!(x), 1)
#define unlikely(x) __builtin_expect(!!(x), 0)
#else
#define likely(x) (x)
#define unlikely(x) (x)
#endif

#ifdef _MSC_VER
#define ALIGNED(n) _declspec(align(n))
#else
#define ALIGNED(n) __attribute__((aligned(n)))
#endif

#define IS_PRINTABLE_ASCII(c) ((unsigned char)(c)-040u < 0137u)

#define CHECK_EOF()           \
    if (buf == buf_end) {     \
        *ret = -2;            \
        return NULL;          \
    }

#define EXPECT_CHAR_NO_CHECK(ch) \
    if (*buf++ != ch) {          \
        *ret = -1;               \
        return NULL;             \
    }

#define EXPECT_CHAR(ch)  \
    CHECK_EOF();         \
    EXPECT_CHAR_NO_CHECK(ch);

#define ADVANCE_TOKEN(tok, toklen)                                     \
    do {                                                               \
        const char *tok_start = buf;                                   \
        static const char ALIGNED(16) ranges2[16] = "\000\040\177\177";\
        int found2;                                                    \
        buf = findchar_fast(buf, buf_end, ranges2, 4, &found2);        \
        if (!found2) { CHECK_EOF(); }                                  \
        while (1) {                                                    \
            if (*buf == ' ') {                                         \
                break;                                                 \
            } else if (unlikely(!IS_PRINTABLE_ASCII(*buf))) {          \
                if ((unsigned char)*buf < '\040' || *buf == '\177') {   \
                    *ret = -1; return NULL;                             \
                }                                                      \
            }                                                          \
            ++buf;                                                     \
            CHECK_EOF();                                               \
        }                                                              \
        tok = tok_start;                                               \
        toklen = buf - tok_start;                                      \
    } while (0)

static const char *token_char_map =
    "\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0"
    "\0\1\0\1\1\1\1\1\0\0\1\1\0\1\1\0\1\1\1\1\1\1\1\1\1\1\0\0\0\0\0\0"
    "\0\1\1\1\1\1\1\1\1\1\1\1\1\1\1\1\1\1\1\1\1\1\1\1\1\1\1\0\0\0\1\1"
    "\1\1\1\1\1\1\1\1\1\1\1\1\1\1\1\1\1\1\1\1\1\1\1\1\1\1\1\0\1\0\1\0"
    "\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0"
    "\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0"
    "\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0"
    "\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";

static const char *findchar_fast(const char *buf, const char *buf_end,
                                  const char *ranges, size_t ranges_size,
                                  int *found)
{
    *found = 0;
    /* SSE4.2 path omitted for portability — the scalar fallback produces
     * identical results and is what matters for diff testing. */
    (void)buf_end;
    (void)ranges;
    (void)ranges_size;
    return buf;
}

static const char *get_token_to_eol(const char *buf, const char *buf_end,
                                     const char **token, size_t *token_len,
                                     int *ret)
{
    const char *token_start = buf;

    /* find non-printable char within the next 8 bytes (hot path) */
    while (likely(buf_end - buf >= 8)) {
#define DOIT()                                      \
    do {                                            \
        if (unlikely(!IS_PRINTABLE_ASCII(*buf)))     \
            goto NonPrintable;                      \
        ++buf;                                      \
    } while (0)
        DOIT(); DOIT(); DOIT(); DOIT();
        DOIT(); DOIT(); DOIT(); DOIT();
#undef DOIT
        continue;
    NonPrintable:
        if ((likely((unsigned char)*buf < '\040') && likely(*buf != '\011')) ||
            unlikely(*buf == '\177')) {
            goto FOUND_CTL;
        }
        ++buf;
    }
    for (;; ++buf) {
        CHECK_EOF();
        if (unlikely(!IS_PRINTABLE_ASCII(*buf))) {
            if ((likely((unsigned char)*buf < '\040') && likely(*buf != '\011')) ||
                unlikely(*buf == '\177')) {
                goto FOUND_CTL;
            }
        }
    }
FOUND_CTL:
    if (likely(*buf == '\015')) {
        ++buf;
        EXPECT_CHAR('\012');
        *token_len = buf - 2 - token_start;
    } else if (*buf == '\012') {
        *token_len = buf - token_start;
        ++buf;
    } else {
        *ret = -1;
        return NULL;
    }
    *token = token_start;
    return buf;
}

static const char *is_complete(const char *buf, const char *buf_end,
                                size_t last_len, int *ret)
{
    int ret_cnt = 0;
    buf = last_len < 3 ? buf : buf + last_len - 3;

    while (1) {
        CHECK_EOF();
        if (*buf == '\015') {
            ++buf;
            CHECK_EOF();
            EXPECT_CHAR('\012');
            ++ret_cnt;
        } else if (*buf == '\012') {
            ++buf;
            ++ret_cnt;
        } else {
            ++buf;
            ret_cnt = 0;
        }
        if (ret_cnt == 2) return buf;
    }
    *ret = -2;
    return NULL;
}

#define PARSE_INT(valp_, mul_)               \
    if (*buf < '0' || '9' < *buf) {          \
        buf++;                               \
        *ret = -1;                           \
        return NULL;                         \
    }                                        \
    *(valp_) = (mul_) * (*buf++ - '0');

#define PARSE_INT_3(valp_)                   \
    do {                                     \
        int res_ = 0;                        \
        PARSE_INT(&res_, 100)                \
        *valp_ = res_;                       \
        PARSE_INT(&res_, 10)                 \
        *valp_ += res_;                      \
        PARSE_INT(&res_, 1)                  \
        *valp_ += res_;                      \
    } while (0)

static const char *parse_token(const char *buf, const char *buf_end,
                                const char **token, size_t *token_len,
                                char next_char, int *ret)
{
    static const char ALIGNED(16) ranges[] =
        "\x00 "  "\"\""  "()"  ",,"  "//"  ":@"  "[]"  "{\xff";
    const char *buf_start = buf;
    int found;
    buf = findchar_fast(buf, buf_end, ranges, sizeof(ranges) - 1, &found);
    if (!found) { CHECK_EOF(); }
    while (1) {
        if (*buf == next_char) {
            break;
        } else if (!token_char_map[(unsigned char)*buf]) {
            *ret = -1;
            return NULL;
        }
        ++buf;
        CHECK_EOF();
    }
    *token = buf_start;
    *token_len = buf - buf_start;
    return buf;
}

static const char *parse_http_version(const char *buf, const char *buf_end,
                                       int *minor_version, int *ret)
{
    if (buf_end - buf < 9) { *ret = -2; return NULL; }
    EXPECT_CHAR_NO_CHECK('H');
    EXPECT_CHAR_NO_CHECK('T');
    EXPECT_CHAR_NO_CHECK('T');
    EXPECT_CHAR_NO_CHECK('P');
    EXPECT_CHAR_NO_CHECK('/');
    EXPECT_CHAR_NO_CHECK('1');
    EXPECT_CHAR_NO_CHECK('.');
    PARSE_INT(minor_version, 1);
    return buf;
}

static const char *parse_headers(const char *buf, const char *buf_end,
                                  struct phr_header *headers,
                                  size_t *num_headers, size_t max_headers,
                                  int *ret)
{
    for (;; ++*num_headers) {
        CHECK_EOF();
        if (*buf == '\015') { ++buf; EXPECT_CHAR('\012'); break; }
        else if (*buf == '\012') { ++buf; break; }
        if (*num_headers == max_headers) { *ret = -1; return NULL; }
        if (!(*num_headers != 0 && (*buf == ' ' || *buf == '\t'))) {
            if ((buf = parse_token(buf, buf_end, &headers[*num_headers].name,
                                   &headers[*num_headers].name_len, ':', ret)) == NULL)
                return NULL;
            if (headers[*num_headers].name_len == 0) { *ret = -1; return NULL; }
            ++buf;
            for (;; ++buf) {
                CHECK_EOF();
                if (!(*buf == ' ' || *buf == '\t')) break;
            }
        } else {
            headers[*num_headers].name = NULL;
            headers[*num_headers].name_len = 0;
        }
        const char *value;
        size_t value_len;
        if ((buf = get_token_to_eol(buf, buf_end, &value, &value_len, ret)) == NULL)
            return NULL;
        const char *value_end = value + value_len;
        for (; value_end != value; --value_end) {
            const char c = *(value_end - 1);
            if (!(c == ' ' || c == '\t')) break;
        }
        headers[*num_headers].value = value;
        headers[*num_headers].value_len = value_end - value;
    }
    return buf;
}

static const char *parse_request(const char *buf, const char *buf_end,
                                  const char **method, size_t *method_len,
                                  const char **path, size_t *path_len,
                                  int *minor_version, struct phr_header *headers,
                                  size_t *num_headers, size_t max_headers,
                                  int *ret)
{
    CHECK_EOF();
    if (*buf == '\015') { ++buf; EXPECT_CHAR('\012'); }
    else if (*buf == '\012') { ++buf; }

    if ((buf = parse_token(buf, buf_end, method, method_len, ' ', ret)) == NULL)
        return NULL;
    do { ++buf; CHECK_EOF(); } while (*buf == ' ');
    ADVANCE_TOKEN(*path, *path_len);
    do { ++buf; CHECK_EOF(); } while (*buf == ' ');
    if (*method_len == 0 || *path_len == 0) { *ret = -1; return NULL; }
    if ((buf = parse_http_version(buf, buf_end, minor_version, ret)) == NULL)
        return NULL;
    if (*buf == '\015') { ++buf; EXPECT_CHAR('\012'); }
    else if (*buf == '\012') { ++buf; }
    else { *ret = -1; return NULL; }

    return parse_headers(buf, buf_end, headers, num_headers, max_headers, ret);
}

static int phr_parse_request(const char *buf_start, size_t len,
                              const char **method, size_t *method_len,
                              const char **path, size_t *path_len,
                              int *minor_version, struct phr_header *headers,
                              size_t *num_headers, size_t last_len)
{
    const char *buf = buf_start, *buf_end = buf_start + len;
    size_t max_headers = *num_headers;
    int r;

    *method = NULL; *method_len = 0;
    *path = NULL; *path_len = 0;
    *minor_version = -1; *num_headers = 0;

    if (last_len != 0 && is_complete(buf, buf_end, last_len, &r) == NULL)
        return r;
    if ((buf = parse_request(buf, buf_end, method, method_len, path, path_len,
                              minor_version, headers, num_headers, max_headers,
                              &r)) == NULL)
        return r;
    return (int)(buf - buf_start);
}

static const char *parse_response(const char *buf, const char *buf_end,
                                   int *minor_version, int *status,
                                   const char **msg, size_t *msg_len,
                                   struct phr_header *headers,
                                   size_t *num_headers, size_t max_headers,
                                   int *ret)
{
    if ((buf = parse_http_version(buf, buf_end, minor_version, ret)) == NULL)
        return NULL;
    if (*buf != ' ') { *ret = -1; return NULL; }
    do { ++buf; CHECK_EOF(); } while (*buf == ' ');
    if (buf_end - buf < 4) { *ret = -2; return NULL; }
    PARSE_INT_3(status);

    if ((buf = get_token_to_eol(buf, buf_end, msg, msg_len, ret)) == NULL)
        return NULL;
    if (*msg_len == 0) {
        /* ok */
    } else if (**msg == ' ') {
        do { ++*msg; --*msg_len; } while (**msg == ' ');
    } else {
        *ret = -1; return NULL;
    }

    return parse_headers(buf, buf_end, headers, num_headers, max_headers, ret);
}

static int phr_parse_response(const char *buf_start, size_t len,
                               int *minor_version, int *status,
                               const char **msg, size_t *msg_len,
                               struct phr_header *headers,
                               size_t *num_headers, size_t last_len)
{
    const char *buf = buf_start, *buf_end = buf + len;
    size_t max_headers = *num_headers;
    int r;

    *minor_version = -1; *status = 0;
    *msg = NULL; *msg_len = 0; *num_headers = 0;

    if (last_len != 0 && is_complete(buf, buf_end, last_len, &r) == NULL)
        return r;
    if ((buf = parse_response(buf, buf_end, minor_version, status, msg, msg_len,
                               headers, num_headers, max_headers, &r)) == NULL)
        return r;
    return (int)(buf - buf_start);
}

static int phr_parse_headers(const char *buf_start, size_t len,
                              struct phr_header *headers, size_t *num_headers,
                              size_t last_len)
{
    const char *buf = buf_start, *buf_end = buf + len;
    size_t max_headers = *num_headers;
    int r;

    *num_headers = 0;

    if (last_len != 0 && is_complete(buf, buf_end, last_len, &r) == NULL)
        return r;
    if ((buf = parse_headers(buf, buf_end, headers, num_headers, max_headers, &r)) == NULL)
        return r;
    return (int)(buf - buf_start);
}

/* --- Chunked transfer encoding decoder --- */

enum {
    CHUNKED_IN_CHUNK_SIZE,
    CHUNKED_IN_CHUNK_EXT,
    CHUNKED_IN_CHUNK_HEADER_EXPECT_LF,
    CHUNKED_IN_CHUNK_DATA,
    CHUNKED_IN_CHUNK_DATA_EXPECT_CR,
    CHUNKED_IN_CHUNK_DATA_EXPECT_LF,
    CHUNKED_IN_TRAILERS_LINE_HEAD,
    CHUNKED_IN_TRAILERS_LINE_MIDDLE
};

static int decode_hex(int ch) {
    if ('0' <= ch && ch <= '9') return ch - '0';
    else if ('A' <= ch && ch <= 'F') return ch - 'A' + 0xa;
    else if ('a' <= ch && ch <= 'f') return ch - 'a' + 0xa;
    else return -1;
}

static ssize_t phr_decode_chunked(struct phr_chunked_decoder *decoder,
                                   char *buf, size_t *_bufsz)
{
    size_t dst = 0, src = 0, bufsz = *_bufsz;
    ssize_t ret = -2;

    decoder->_total_read += bufsz;

    while (1) {
        switch (decoder->_state) {
        case CHUNKED_IN_CHUNK_SIZE:
            for (;; ++src) {
                int v;
                if (src == bufsz) goto Exit;
                if ((v = decode_hex(buf[src])) == -1) {
                    if (decoder->_hex_count == 0) { ret = -1; goto Exit; }
                    switch (buf[src]) {
                    case ' ': case '\011': case ';': case '\012': case '\015':
                        break;
                    default:
                        ret = -1; goto Exit;
                    }
                    break;
                }
                if (decoder->_hex_count == (char)(sizeof(size_t) * 2)) {
                    ret = -1; goto Exit;
                }
                decoder->bytes_left_in_chunk = decoder->bytes_left_in_chunk * 16 + v;
                ++decoder->_hex_count;
            }
            decoder->_hex_count = 0;
            decoder->_state = CHUNKED_IN_CHUNK_EXT;
            /* fallthru */
        case CHUNKED_IN_CHUNK_EXT:
            for (;; ++src) {
                if (src == bufsz) goto Exit;
                if (buf[src] == '\015') break;
                else if (buf[src] == '\012') { ret = -1; goto Exit; }
            }
            ++src;
            decoder->_state = CHUNKED_IN_CHUNK_HEADER_EXPECT_LF;
            /* fallthru */
        case CHUNKED_IN_CHUNK_HEADER_EXPECT_LF:
            if (src == bufsz) goto Exit;
            if (buf[src] != '\012') { ret = -1; goto Exit; }
            ++src;
            if (decoder->bytes_left_in_chunk == 0) {
                if (decoder->consume_trailer) {
                    decoder->_state = CHUNKED_IN_TRAILERS_LINE_HEAD;
                    break;
                } else {
                    goto Complete;
                }
            }
            decoder->_state = CHUNKED_IN_CHUNK_DATA;
            /* fallthru */
        case CHUNKED_IN_CHUNK_DATA: {
            size_t avail = bufsz - src;
            if (avail < decoder->bytes_left_in_chunk) {
                if (dst != src) memmove(buf + dst, buf + src, avail);
                src += avail; dst += avail;
                decoder->bytes_left_in_chunk -= avail;
                goto Exit;
            }
            if (dst != src)
                memmove(buf + dst, buf + src, decoder->bytes_left_in_chunk);
            src += decoder->bytes_left_in_chunk;
            dst += decoder->bytes_left_in_chunk;
            decoder->bytes_left_in_chunk = 0;
            decoder->_state = CHUNKED_IN_CHUNK_DATA_EXPECT_CR;
        }
            /* fallthru */
        case CHUNKED_IN_CHUNK_DATA_EXPECT_CR:
            if (src == bufsz) goto Exit;
            if (buf[src] != '\015') { ret = -1; goto Exit; }
            ++src;
            decoder->_state = CHUNKED_IN_CHUNK_DATA_EXPECT_LF;
            /* fallthru */
        case CHUNKED_IN_CHUNK_DATA_EXPECT_LF:
            if (src == bufsz) goto Exit;
            if (buf[src] != '\012') { ret = -1; goto Exit; }
            ++src;
            decoder->_state = CHUNKED_IN_CHUNK_SIZE;
            break;
        case CHUNKED_IN_TRAILERS_LINE_HEAD:
            for (;; ++src) {
                if (src == bufsz) goto Exit;
                if (buf[src] != '\015') break;
            }
            if (buf[src++] == '\012') goto Complete;
            decoder->_state = CHUNKED_IN_TRAILERS_LINE_MIDDLE;
            /* fallthru */
        case CHUNKED_IN_TRAILERS_LINE_MIDDLE:
            for (;; ++src) {
                if (src == bufsz) goto Exit;
                if (buf[src] == '\012') break;
            }
            ++src;
            decoder->_state = CHUNKED_IN_TRAILERS_LINE_HEAD;
            break;
        default:
            assert(!"decoder is corrupt");
        }
    }

Complete:
    ret = bufsz - src;
Exit:
    if (dst != src) memmove(buf + dst, buf + src, bufsz - src);
    *_bufsz = dst;
    if (ret == -2) {
        decoder->_total_overhead += bufsz - dst;
        if (decoder->_total_overhead >= 100 * 1024 &&
            decoder->_total_read - decoder->_total_overhead < decoder->_total_read / 4)
            ret = -1;
    }
    return ret;
}

static int phr_decode_chunked_is_in_data(struct phr_chunked_decoder *decoder) {
    return decoder->_state == CHUNKED_IN_CHUNK_DATA;
}

#undef CHECK_EOF
#undef EXPECT_CHAR
#undef EXPECT_CHAR_NO_CHECK
#undef ADVANCE_TOKEN

/* ================================================================
 * SECTION 3: Tests (adapted from test.c)
 * ================================================================ */

static int bufis(const char *s, size_t l, const char *t) {
    return strlen(t) == l && memcmp(s, t, l) == 0;
}

/* Input buffer: a large heap buffer. The original test.c uses mmap guard
 * pages to detect overreads; we replace that with a simple malloc buffer
 * since Rust memory safety makes the guard-page trick unnecessary. */
#define INPUT_BUF_SIZE 4096
static char input_storage[INPUT_BUF_SIZE];
static char *inputbuf; /* points to the END of input_storage */

static void test_request(void) {
    const char *method;
    size_t method_len;
    const char *path;
    size_t path_len;
    int minor_version;
    struct phr_header headers[4];
    size_t num_headers;

#define PARSE(s, last_len, exp, comment)                                   \
    do {                                                                   \
        size_t slen = sizeof(s) - 1;                                       \
        note(comment);                                                     \
        num_headers = sizeof(headers) / sizeof(headers[0]);                \
        memcpy(inputbuf - slen, s, slen);                                  \
        ok(phr_parse_request(inputbuf - slen, slen, &method, &method_len,  \
                             &path, &path_len, &minor_version, headers,    \
                             &num_headers, last_len) ==                    \
           (exp == 0 ? (int)slen : exp));                                  \
    } while (0)

    PARSE("GET / HTTP/1.0\r\n\r\n", 0, 0, "simple");
    ok(num_headers == 0);
    ok(bufis(method, method_len, "GET"));
    ok(bufis(path, path_len, "/"));
    ok(minor_version == 0);

    PARSE("GET / HTTP/1.0\r\n\r", 0, -2, "partial");

    PARSE("GET /hoge HTTP/1.1\r\nHost: example.com\r\nCookie: \r\n\r\n", 0, 0, "parse headers");
    ok(num_headers == 2);
    ok(bufis(method, method_len, "GET"));
    ok(bufis(path, path_len, "/hoge"));
    ok(minor_version == 1);
    ok(bufis(headers[0].name, headers[0].name_len, "Host"));
    ok(bufis(headers[0].value, headers[0].value_len, "example.com"));
    ok(bufis(headers[1].name, headers[1].name_len, "Cookie"));
    ok(bufis(headers[1].value, headers[1].value_len, ""));

    PARSE("GET /hoge HTTP/1.1\r\nHost: example.com\r\nUser-Agent: \343\201\262\343/1.0\r\n\r\n", 0, 0, "multibyte included");
    ok(num_headers == 2);
    ok(bufis(method, method_len, "GET"));
    ok(bufis(path, path_len, "/hoge"));
    ok(minor_version == 1);
    ok(bufis(headers[0].name, headers[0].name_len, "Host"));
    ok(bufis(headers[0].value, headers[0].value_len, "example.com"));
    ok(bufis(headers[1].name, headers[1].name_len, "User-Agent"));
    ok(bufis(headers[1].value, headers[1].value_len, "\343\201\262\343/1.0"));

    PARSE("GET / HTTP/1.0\r\nfoo: \r\nfoo: b\r\n  \tc\r\n\r\n", 0, 0, "parse multiline");
    ok(num_headers == 3);
    ok(bufis(method, method_len, "GET"));
    ok(bufis(path, path_len, "/"));
    ok(minor_version == 0);
    ok(bufis(headers[0].name, headers[0].name_len, "foo"));
    ok(bufis(headers[0].value, headers[0].value_len, ""));
    ok(bufis(headers[1].name, headers[1].name_len, "foo"));
    ok(bufis(headers[1].value, headers[1].value_len, "b"));
    ok(headers[2].name == NULL);
    ok(bufis(headers[2].value, headers[2].value_len, "  \tc"));

    PARSE("GET / HTTP/1.0\r\nfoo : ab\r\n\r\n", 0, -1, "parse header name with trailing space");

    PARSE("GET", 0, -2, "incomplete 1");
    ok(method == NULL);
    PARSE("GET ", 0, -2, "incomplete 2");
    ok(bufis(method, method_len, "GET"));
    PARSE("GET /", 0, -2, "incomplete 3");
    ok(path == NULL);
    PARSE("GET / ", 0, -2, "incomplete 4");
    ok(bufis(path, path_len, "/"));
    PARSE("GET / H", 0, -2, "incomplete 5");
    PARSE("GET / HTTP/1.", 0, -2, "incomplete 6");
    PARSE("GET / HTTP/1.0", 0, -2, "incomplete 7");
    ok(minor_version == -1);
    PARSE("GET / HTTP/1.0\r", 0, -2, "incomplete 8");
    ok(minor_version == 0);

    PARSE("GET /hoge HTTP/1.0\r\n\r", strlen("GET /hoge HTTP/1.0\r\n\r") - 1, -2, "slowloris (incomplete)");
    PARSE("GET /hoge HTTP/1.0\r\n\r\n", strlen("GET /hoge HTTP/1.0\r\n\r\n") - 1, 0, "slowloris (complete)");

    PARSE(" / HTTP/1.0\r\n\r\n", 0, -1, "empty method");
    PARSE("GET  HTTP/1.0\r\n\r\n", 0, -1, "empty request-target");

    PARSE("GET / HTTP/1.0\r\n:a\r\n\r\n", 0, -1, "empty header name");
    PARSE("GET / HTTP/1.0\r\n :a\r\n\r\n", 0, -1, "header name (space only)");

    PARSE("G\0T / HTTP/1.0\r\n\r\n", 0, -1, "NUL in method");
    PARSE("G\tT / HTTP/1.0\r\n\r\n", 0, -1, "tab in method");
    PARSE(":GET / HTTP/1.0\r\n\r\n", 0, -1, "invalid method");
    PARSE("GET /\x7fhello HTTP/1.0\r\n\r\n", 0, -1, "DEL in uri-path");
    PARSE("GET / HTTP/1.0\r\na\0b: c\r\n\r\n", 0, -1, "NUL in header name");
    PARSE("GET / HTTP/1.0\r\nab: c\0d\r\n\r\n", 0, -1, "NUL in header value");
    PARSE("GET / HTTP/1.0\r\na\033b: c\r\n\r\n", 0, -1, "CTL in header name");
    PARSE("GET / HTTP/1.0\r\nab: c\033\r\n\r\n", 0, -1, "CTL in header value");
    PARSE("GET / HTTP/1.0\r\n/: 1\r\n\r\n", 0, -1, "invalid char in header value");
    PARSE("GET /\xa0 HTTP/1.0\r\nh: c\xa2y\r\n\r\n", 0, 0, "accept MSB chars");
    ok(num_headers == 1);
    ok(bufis(method, method_len, "GET"));
    ok(bufis(path, path_len, "/\xa0"));
    ok(minor_version == 0);
    ok(bufis(headers[0].name, headers[0].name_len, "h"));
    ok(bufis(headers[0].value, headers[0].value_len, "c\xa2y"));

    PARSE("GET / HTTP/1.0\r\n\x7c\x7e: 1\r\n\r\n", 0, 0, "accept |~ (though forbidden by SSE)");
    ok(num_headers == 1);
    ok(bufis(headers[0].name, headers[0].name_len, "\x7c\x7e"));
    ok(bufis(headers[0].value, headers[0].value_len, "1"));

    PARSE("GET / HTTP/1.0\r\n\x7b: 1\r\n\r\n", 0, -1, "disallow {");

    PARSE("GET / HTTP/1.0\r\nfoo: a \t \r\n\r\n", 0, 0, "exclude leading and trailing spaces in header value");
    ok(bufis(headers[0].value, headers[0].value_len, "a"));

    PARSE("GET   /   HTTP/1.0\r\n\r\n", 0, 0, "accept multiple spaces between tokens");

#undef PARSE
}

static void test_response(void) {
    int minor_version;
    int status;
    const char *msg;
    size_t msg_len;
    struct phr_header headers[4];
    size_t num_headers;

#define PARSE(s, last_len, exp, comment)                                   \
    do {                                                                   \
        size_t slen = sizeof(s) - 1;                                       \
        note(comment);                                                     \
        num_headers = sizeof(headers) / sizeof(headers[0]);                \
        memcpy(inputbuf - slen, s, slen);                                  \
        ok(phr_parse_response(inputbuf - slen, slen, &minor_version,       \
                              &status, &msg, &msg_len, headers,            \
                              &num_headers, last_len) ==                   \
           (exp == 0 ? (int)slen : exp));                                  \
    } while (0)

    PARSE("HTTP/1.0 200 OK\r\n\r\n", 0, 0, "simple");
    ok(num_headers == 0);
    ok(status == 200);
    ok(minor_version == 0);
    ok(bufis(msg, msg_len, "OK"));

    PARSE("HTTP/1.0 200 OK\r\n\r", 0, -2, "partial");

    PARSE("HTTP/1.1 200 OK\r\nHost: example.com\r\nCookie: \r\n\r\n", 0, 0, "parse headers");
    ok(num_headers == 2);
    ok(minor_version == 1);
    ok(status == 200);
    ok(bufis(msg, msg_len, "OK"));
    ok(bufis(headers[0].name, headers[0].name_len, "Host"));
    ok(bufis(headers[0].value, headers[0].value_len, "example.com"));
    ok(bufis(headers[1].name, headers[1].name_len, "Cookie"));
    ok(bufis(headers[1].value, headers[1].value_len, ""));

    PARSE("HTTP/1.0 200 OK\r\nfoo: \r\nfoo: b\r\n  \tc\r\n\r\n", 0, 0, "parse multiline");
    ok(num_headers == 3);
    ok(minor_version == 0);
    ok(status == 200);
    ok(bufis(msg, msg_len, "OK"));
    ok(bufis(headers[0].name, headers[0].name_len, "foo"));
    ok(bufis(headers[0].value, headers[0].value_len, ""));
    ok(bufis(headers[1].name, headers[1].name_len, "foo"));
    ok(bufis(headers[1].value, headers[1].value_len, "b"));
    ok(headers[2].name == NULL);
    ok(bufis(headers[2].value, headers[2].value_len, "  \tc"));

    PARSE("HTTP/1.0 500 Internal Server Error\r\n\r\n", 0, 0, "internal server error");
    ok(num_headers == 0);
    ok(minor_version == 0);
    ok(status == 500);
    ok(bufis(msg, msg_len, "Internal Server Error"));
    ok(msg_len == sizeof("Internal Server Error") - 1);

    PARSE("H", 0, -2, "incomplete 1");
    PARSE("HTTP/1.", 0, -2, "incomplete 2");
    PARSE("HTTP/1.1", 0, -2, "incomplete 3");
    ok(minor_version == -1);
    PARSE("HTTP/1.1 ", 0, -2, "incomplete 4");
    ok(minor_version == 1);
    PARSE("HTTP/1.1 2", 0, -2, "incomplete 5");
    PARSE("HTTP/1.1 200", 0, -2, "incomplete 6");
    ok(status == 0);
    PARSE("HTTP/1.1 200 ", 0, -2, "incomplete 7");
    ok(status == 200);
    PARSE("HTTP/1.1 200 O", 0, -2, "incomplete 8");
    PARSE("HTTP/1.1 200 OK\r", 0, -2, "incomplete 9");
    ok(msg == NULL);
    PARSE("HTTP/1.1 200 OK\r\n", 0, -2, "incomplete 10");
    ok(bufis(msg, msg_len, "OK"));
    PARSE("HTTP/1.1 200 OK\n", 0, -2, "incomplete 11");
    ok(bufis(msg, msg_len, "OK"));

    PARSE("HTTP/1.1 200 OK\r\nA: 1\r", 0, -2, "incomplete 11");
    ok(num_headers == 0);
    PARSE("HTTP/1.1 200 OK\r\nA: 1\r\n", 0, -2, "incomplete 12");
    ok(num_headers == 1);
    ok(bufis(headers[0].name, headers[0].name_len, "A"));
    ok(bufis(headers[0].value, headers[0].value_len, "1"));

    PARSE("HTTP/1.0 200 OK\r\n\r", strlen("HTTP/1.0 200 OK\r\n\r") - 1, -2, "slowloris (incomplete)");
    PARSE("HTTP/1.0 200 OK\r\n\r\n", strlen("HTTP/1.0 200 OK\r\n\r\n") - 1, 0, "slowloris (complete)");

    PARSE("HTTP/1. 200 OK\r\n\r\n", 0, -1, "invalid http version");
    PARSE("HTTP/1.2z 200 OK\r\n\r\n", 0, -1, "invalid http version 2");
    PARSE("HTTP/1.1  OK\r\n\r\n", 0, -1, "no status code");

    PARSE("HTTP/1.1 200\r\n\r\n", 0, 0, "accept missing trailing whitespace in status-line");
    ok(bufis(msg, msg_len, ""));
    PARSE("HTTP/1.1 200X\r\n\r\n", 0, -1, "garbage after status 1");
    PARSE("HTTP/1.1 200X \r\n\r\n", 0, -1, "garbage after status 2");
    PARSE("HTTP/1.1 200X OK\r\n\r\n", 0, -1, "garbage after status 3");

    PARSE("HTTP/1.1 200 OK\r\nbar: \t b\t \t\r\n\r\n", 0, 0, "exclude leading and trailing spaces in header value");
    ok(bufis(headers[0].value, headers[0].value_len, "b"));

    PARSE("HTTP/1.1   200   OK\r\n\r\n", 0, 0, "accept multiple spaces between tokens");

#undef PARSE
}

static void test_headers(void) {
    struct phr_header headers[4];
    size_t num_headers;

#define PARSE(s, last_len, exp, comment)                                   \
    do {                                                                   \
        note(comment);                                                     \
        num_headers = sizeof(headers) / sizeof(headers[0]);                \
        ok(phr_parse_headers(s, strlen(s), headers, &num_headers,          \
                             last_len) == (exp == 0 ? (int)strlen(s) : exp)); \
    } while (0)

    PARSE("Host: example.com\r\nCookie: \r\n\r\n", 0, 0, "simple");
    ok(num_headers == 2);
    ok(bufis(headers[0].name, headers[0].name_len, "Host"));
    ok(bufis(headers[0].value, headers[0].value_len, "example.com"));
    ok(bufis(headers[1].name, headers[1].name_len, "Cookie"));
    ok(bufis(headers[1].value, headers[1].value_len, ""));

    PARSE("Host: example.com\r\nCookie: \r\n\r\n", 1, 0, "slowloris");
    ok(num_headers == 2);
    ok(bufis(headers[0].name, headers[0].name_len, "Host"));
    ok(bufis(headers[0].value, headers[0].value_len, "example.com"));
    ok(bufis(headers[1].name, headers[1].name_len, "Cookie"));
    ok(bufis(headers[1].value, headers[1].value_len, ""));

    PARSE("Host: example.com\r\nCookie: \r\n\r", 0, -2, "partial");

    PARSE("Host: e\7fample.com\r\nCookie: \r\n\r", 0, -1, "error");

#undef PARSE
}

static void test_chunked_at_once(int line, int consume_trailer,
                                  const char *encoded, const char *decoded,
                                  ssize_t expected)
{
    struct phr_chunked_decoder dec = {0};
    char *buf;
    size_t bufsz;
    ssize_t ret;

    dec.consume_trailer = consume_trailer;
    note("testing at-once");

    buf = strdup(encoded);
    bufsz = strlen(buf);

    ret = phr_decode_chunked(&dec, buf, &bufsz);

    ok(ret == expected);
    ok(bufsz == strlen(decoded));
    ok(bufis(buf, bufsz, decoded));
    if (expected >= 0) {
        if (ret == expected)
            ok(bufis(buf + bufsz, ret, encoded + strlen(encoded) - ret));
        else
            ok(0);
    }
    free(buf);
}

static void test_chunked_per_byte(int line, int consume_trailer,
                                   const char *encoded, const char *decoded,
                                   ssize_t expected)
{
    struct phr_chunked_decoder dec = {0};
    char *buf = malloc(strlen(encoded) + 1);
    size_t bytes_to_consume = strlen(encoded) - (expected >= 0 ? expected : 0);
    size_t bytes_ready = 0, bufsz, i;
    ssize_t ret;

    dec.consume_trailer = consume_trailer;
    note("testing per-byte");

    for (i = 0; i < bytes_to_consume - 1; ++i) {
        buf[bytes_ready] = encoded[i];
        bufsz = 1;
        ret = phr_decode_chunked(&dec, buf + bytes_ready, &bufsz);
        if (ret != -2) { ok(0); goto cleanup; }
        bytes_ready += bufsz;
    }
    strcpy(buf + bytes_ready, encoded + bytes_to_consume - 1);
    bufsz = strlen(buf + bytes_ready);
    ret = phr_decode_chunked(&dec, buf + bytes_ready, &bufsz);
    ok(ret == expected);
    bytes_ready += bufsz;
    ok(bytes_ready == strlen(decoded));
    ok(bufis(buf, bytes_ready, decoded));
    if (expected >= 0) {
        if (ret == expected)
            ok(bufis(buf + bytes_ready, expected, encoded + bytes_to_consume));
        else
            ok(0);
    }

cleanup:
    free(buf);
}

static void test_chunked_failure(int line, const char *encoded, ssize_t expected) {
    struct phr_chunked_decoder dec = {0};
    char *buf = strdup(encoded);
    size_t bufsz, i;
    ssize_t ret;

    note("testing failure at-once");
    bufsz = strlen(buf);
    ret = phr_decode_chunked(&dec, buf, &bufsz);
    ok(ret == expected);

    note("testing failure per-byte");
    memset(&dec, 0, sizeof(dec));
    for (i = 0; encoded[i] != '\0'; ++i) {
        buf[0] = encoded[i];
        bufsz = 1;
        ret = phr_decode_chunked(&dec, buf, &bufsz);
        if (ret == -1) { ok(ret == expected); goto cleanup; }
        else if (ret == -2) { /* continue */ }
        else { ok(0); goto cleanup; }
    }
    ok(ret == expected);

cleanup:
    free(buf);
}

static void (*chunked_test_runners[])(int, int, const char *, const char *, ssize_t) = {
    test_chunked_at_once, test_chunked_per_byte, NULL
};

static void test_chunked(void) {
    size_t i;
    for (i = 0; chunked_test_runners[i] != NULL; ++i) {
        chunked_test_runners[i](__LINE__, 0, "b\r\nhello world\r\n0\r\n", "hello world", 0);
        chunked_test_runners[i](__LINE__, 0, "6\r\nhello \r\n5\r\nworld\r\n0\r\n", "hello world", 0);
        chunked_test_runners[i](__LINE__, 0, "6;comment=hi\r\nhello \r\n5\r\nworld\r\n0\r\n", "hello world", 0);
        chunked_test_runners[i](__LINE__, 0, "6 ; comment\r\nhello \r\n5\r\nworld\r\n0\r\n", "hello world", 0);
        chunked_test_runners[i](__LINE__, 0, "6\r\nhello \r\n5\r\nworld\r\n0\r\na: b\r\nc: d\r\n\r\n", "hello world",
                                (ssize_t)(sizeof("a: b\r\nc: d\r\n\r\n") - 1));
        chunked_test_runners[i](__LINE__, 0, "b\r\nhello world\r\n0\r\n", "hello world", 0);
    }

    note("failures");
    test_chunked_failure(__LINE__, "z\r\nabcdefg", -1);
    if (sizeof(size_t) == 8) {
        test_chunked_failure(__LINE__, "6\r\nhello \r\nffffffffffffffff\r\nabcdefg", -2);
        test_chunked_failure(__LINE__, "6\r\nhello \r\nfffffffffffffffff\r\nabcdefg", -1);
    }
    test_chunked_failure(__LINE__, "1x\r\na\r\n0\r\n", -1);

    test_chunked_failure(__LINE__, "6\nhello \r\n5\r\nworld\r\n0\r\n", -1);
    test_chunked_failure(__LINE__, "6\r\nhello \n5\r\nworld\r\n0\r\n", -1);
    test_chunked_failure(__LINE__, "6\r\nhello \r\n5\r\nworld\n0\r\n", -1);
    test_chunked_failure(__LINE__, "6\r\nhello \r\n5\r\nworld\n0\r\n", -1);
    test_chunked_failure(__LINE__, "6\r\nhello \r\n5\r\nworld\r\n0\n", -1);
    test_chunked_failure(__LINE__, "6\rX\nhello \n5\r\nworld\r\n0\r\n", -1);
}

static void test_chunked_consume_trailer(void) {
    size_t i;
    for (i = 0; chunked_test_runners[i] != NULL; ++i) {
        chunked_test_runners[i](__LINE__, 1, "b\r\nhello world\r\n0\r\n", "hello world", -2);
        chunked_test_runners[i](__LINE__, 1, "6\r\nhello \r\n5\r\nworld\r\n0\r\n", "hello world", -2);
        chunked_test_runners[i](__LINE__, 1, "6;comment=hi\r\nhello \r\n5\r\nworld\r\n0\r\n", "hello world", -2);
        chunked_test_runners[i](__LINE__, 1, "b\r\nhello world\r\n0\r\n\r\n", "hello world", 0);
        chunked_test_runners[i](__LINE__, 1, "6\r\nhello \r\n5\r\nworld\r\n0\r\na: b\r\nc: d\r\n\r\n", "hello world", 0);
        chunked_test_runners[i](__LINE__, 1, "b\r\nhello world\r\n0\r\n\n", "hello world", 0);
        chunked_test_runners[i](__LINE__, 1, "6\r\nhello \r\n5\r\nworld\r\n0\r\na: b\nc: d\n\n", "hello world", 0);
    }
}

static void test_chunked_leftdata(void) {
#define NEXT_REQ "GET / HTTP/1.1\r\n\r\n"
    struct phr_chunked_decoder dec = {0};
    dec.consume_trailer = 1;
    char buf[] = "5\r\nabcde\r\n0\r\n\r\n" NEXT_REQ;
    size_t bufsz = sizeof(buf) - 1;

    ssize_t ret = phr_decode_chunked(&dec, buf, &bufsz);
    ok(ret >= 0);
    ok(bufsz == 5);
    ok(memcmp(buf, "abcde", 5) == 0);
    ok(ret == sizeof(NEXT_REQ) - 1);
    ok(memcmp(buf + bufsz, NEXT_REQ, sizeof(NEXT_REQ) - 1) == 0);
#undef NEXT_REQ
}

static ssize_t do_test_chunked_overhead(size_t chunk_len, size_t chunk_count,
                                         const char *extra)
{
    struct phr_chunked_decoder dec = {0};
    char buf[1024];
    size_t bufsz;
    ssize_t ret;

    for (size_t i = 0; i < chunk_count; ++i) {
        bufsz = (size_t)sprintf(buf, "%zx%s\r\n", chunk_len, extra);
        if ((ret = phr_decode_chunked(&dec, buf, &bufsz)) != -2)
            goto Exit;
        assert(bufsz == 0);
        memset(buf, 'A', chunk_len);
        bufsz = chunk_len;
        if ((ret = phr_decode_chunked(&dec, buf, &bufsz)) != -2)
            goto Exit;
        assert(bufsz == chunk_len);
        strcpy(buf, "\r\n");
        bufsz = 2;
        if ((ret = phr_decode_chunked(&dec, buf, &bufsz)) != -2)
            goto Exit;
        assert(bufsz == 0);
    }

    strcpy(buf, "0\r\n\r\n");
    bufsz = 5;
    ret = phr_decode_chunked(&dec, buf, &bufsz);
    assert(bufsz == 0);

Exit:
    return ret;
}

static void test_chunked_overhead(void) {
    ok(do_test_chunked_overhead(100, 10000, "") == 2);
    ok(do_test_chunked_overhead(10, 100000, "") == 2);
    ok(do_test_chunked_overhead(1, 1000000, "") == -1);
    ok(do_test_chunked_overhead(10, 100000, "; tiny=1") == 2);
    ok(do_test_chunked_overhead(10, 100000, "; large=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa") == -1);
}

int main(void) {
    inputbuf = input_storage + INPUT_BUF_SIZE;

    subtest("request", test_request);
    subtest("response", test_response);
    subtest("headers", test_headers);
    subtest("chunked", test_chunked);
    subtest("chunked-consume-trailer", test_chunked_consume_trailer);
    subtest("chunked-leftdata", test_chunked_leftdata);
    subtest("chunked-overhead", test_chunked_overhead);

    return done_testing();
}
