// Fibonacci - iterative with while loop
#include <stdio.h>

int fibonacci(int n) {
    if (n <= 0) {
        return 0;
    }
    if (n == 1) {
        return 1;
    }
    int a = 0;
    int b = 1;
    int i = 2;
    while (i <= n) {
        int temp = a + b;
        a = b;
        b = temp;
        i++;
    }
    return b;
}

int main(void) {
    printf("%d\n", fibonacci(0));
    printf("%d\n", fibonacci(1));
    printf("%d\n", fibonacci(5));
    printf("%d\n", fibonacci(10));
    printf("%d\n", fibonacci(20));
    return 0;
}
