// Integer power - for loop with multiplication
#include <stdio.h>

int power(int base, int exp) {
    int result = 1;
    for (int i = 0; i < exp; i++) {
        result *= base;
    }
    return result;
}

int is_even(int n) {
    return n % 2 == 0;
}

int abs_val(int x) {
    if (x < 0) {
        return -x;
    }
    return x;
}

int main(void) {
    printf("%d\n", power(2, 10));
    printf("%d\n", power(3, 5));
    printf("%d\n", power(1, 100));
    printf("%d\n", power(5, 0));
    printf("%d\n", is_even(4));
    printf("%d\n", is_even(7));
    printf("%d\n", abs_val(-42));
    printf("%d\n", abs_val(42));
    return 0;
}
