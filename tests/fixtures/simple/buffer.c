// Buffer operations - medium difficulty (bounded array access)
#include <stdio.h>
#include <string.h>

#define BUFFER_SIZE 256

typedef struct {
    char data[BUFFER_SIZE];
    int len;
} Buffer;

void buffer_init(Buffer *buf) {
    buf->len = 0;
    buf->data[0] = '\0';
}

int buffer_append(Buffer *buf, const char *str) {
    int slen = strlen(str);
    if (buf->len + slen >= BUFFER_SIZE) {
        return -1;  // would overflow
    }
    memcpy(buf->data + buf->len, str, slen);
    buf->len += slen;
    buf->data[buf->len] = '\0';
    return 0;
}

int main(void) {
    Buffer buf;
    buffer_init(&buf);
    buffer_append(&buf, "Hello");
    buffer_append(&buf, ", ");
    buffer_append(&buf, "Noricum!");
    printf("%s (len=%d)\n", buf.data, buf.len);
    return 0;
}
