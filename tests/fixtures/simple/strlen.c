// Custom strlen - medium difficulty (pointer traversal)
#include <stdio.h>

int my_strlen(const char *s) {
    int len = 0;
    while (*s != '\0') {
        len++;
        s++;
    }
    return len;
}

int main(void) {
    printf("%d\n", my_strlen("hello"));
    printf("%d\n", my_strlen(""));
    printf("%d\n", my_strlen("noricum"));
    return 0;
}
