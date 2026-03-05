// Factorial - for loop pattern
#include <stdio.h>

int factorial(int n) {
    int result = 1;
    for (int i = 2; i <= n; i++) {
        result *= i;
    }
    return result;
}

int main(void) {
    printf("%d\n", factorial(0));
    printf("%d\n", factorial(1));
    printf("%d\n", factorial(5));
    printf("%d\n", factorial(10));
    return 0;
}
