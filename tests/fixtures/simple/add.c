// Simple addition function - easiest migration case
#include <stdio.h>

int add(int a, int b) {
    return a + b;
}

int main(void) {
    printf("%d\n", add(2, 3));
    printf("%d\n", add(-1, 1));
    printf("%d\n", add(0, 0));
    return 0;
}
