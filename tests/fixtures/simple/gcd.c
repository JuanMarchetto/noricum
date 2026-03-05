// GCD using Euclidean algorithm - while loop with modulo
#include <stdio.h>

int gcd(int a, int b) {
    while (b != 0) {
        int temp = b;
        b = a % b;
        a = temp;
    }
    return a;
}

int lcm(int a, int b) {
    return a / gcd(a, b) * b;
}

int main(void) {
    printf("%d\n", gcd(12, 8));
    printf("%d\n", gcd(100, 75));
    printf("%d\n", gcd(7, 13));
    printf("%d\n", lcm(4, 6));
    printf("%d\n", lcm(3, 5));
    return 0;
}
