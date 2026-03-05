// Max and min with ternary operators and if/else
#include <stdio.h>

int max(int a, int b) {
    return a > b ? a : b;
}

int min(int a, int b) {
    return a < b ? a : b;
}

int clamp(int val, int lo, int hi) {
    if (val < lo) {
        return lo;
    } else if (val > hi) {
        return hi;
    } else {
        return val;
    }
}

int main(void) {
    printf("%d\n", max(3, 7));
    printf("%d\n", max(-1, -5));
    printf("%d\n", min(3, 7));
    printf("%d\n", min(-1, -5));
    printf("%d\n", clamp(5, 0, 10));
    printf("%d\n", clamp(-3, 0, 10));
    printf("%d\n", clamp(15, 0, 10));
    return 0;
}
