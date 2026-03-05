// Error code pattern - common C idiom to migrate to Rust Result
#include <stdio.h>

typedef enum {
    OK = 0,
    ERR_NULL_PTR = -1,
    ERR_OUT_OF_RANGE = -2,
    ERR_OVERFLOW = -3,
} ErrorCode;

ErrorCode safe_divide(int a, int b, int *result) {
    if (!result) return ERR_NULL_PTR;
    if (b == 0) return ERR_OUT_OF_RANGE;
    *result = a / b;
    return OK;
}

ErrorCode safe_add(int a, int b, int *result) {
    if (!result) return ERR_NULL_PTR;
    // Check for overflow
    if ((b > 0 && a > __INT_MAX__ - b) ||
        (b < 0 && a < (-__INT_MAX__ - 1) - b)) {
        return ERR_OVERFLOW;
    }
    *result = a + b;
    return OK;
}

int main(void) {
    int result;
    ErrorCode err;

    err = safe_divide(10, 3, &result);
    printf("10/3 = %d (err=%d)\n", result, err);

    err = safe_divide(10, 0, &result);
    printf("10/0 err=%d\n", err);

    err = safe_add(2147483647, 1, &result);
    printf("INT_MAX+1 err=%d\n", err);

    return 0;
}
