/*
 * cJSON full migration test — exercises core API for diff testing.
 * Covers: parsing, printing, creation, manipulation, query, compare, minify.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "cJSON.h"

/* ---- Test helpers ---- */
static int test_count = 0;
static int pass_count = 0;

static void check(int ok, const char *label) {
    test_count++;
    if (ok) {
        pass_count++;
        printf("PASS: %s\n", label);
    } else {
        printf("FAIL: %s\n", label);
    }
}

/* ---- 1. Parse + print round-trip ---- */
static void test_parse_print(void) {
    const char *json =
        "{\"name\":\"Noricum\",\"version\":1.7,\"active\":true,"
        "\"tags\":[\"rust\",\"migration\"],\"meta\":null}";

    cJSON *root = cJSON_Parse(json);
    check(root != NULL, "parse basic object");

    /* Formatted print */
    char *printed = cJSON_Print(root);
    check(printed != NULL, "print formatted");
    printf("Formatted:\n%s\n", printed);
    cJSON_free(printed);

    /* Unformatted print */
    char *unformatted = cJSON_PrintUnformatted(root);
    check(unformatted != NULL, "print unformatted");
    printf("Unformatted: %s\n", unformatted);
    cJSON_free(unformatted);

    cJSON_Delete(root);
}

/* ---- 2. Type checking ---- */
static void test_type_checks(void) {
    cJSON *root = cJSON_Parse(
        "{\"s\":\"hello\",\"n\":42,\"b\":true,\"f\":false,\"null\":null,"
        "\"a\":[1,2],\"o\":{\"x\":1}}");
    check(root != NULL, "parse for type checks");

    check(cJSON_IsString(cJSON_GetObjectItem(root, "s")), "is_string");
    check(cJSON_IsNumber(cJSON_GetObjectItem(root, "n")), "is_number");
    check(cJSON_IsTrue(cJSON_GetObjectItem(root, "b")), "is_true");
    check(cJSON_IsFalse(cJSON_GetObjectItem(root, "f")), "is_false");
    check(cJSON_IsBool(cJSON_GetObjectItem(root, "b")), "is_bool");
    check(cJSON_IsNull(cJSON_GetObjectItem(root, "null")), "is_null");
    check(cJSON_IsArray(cJSON_GetObjectItem(root, "a")), "is_array");
    check(cJSON_IsObject(cJSON_GetObjectItem(root, "o")), "is_object");

    cJSON_Delete(root);
}

/* ---- 3. Getters ---- */
static void test_getters(void) {
    cJSON *root = cJSON_Parse("{\"name\":\"test\",\"val\":3.14}");
    check(root != NULL, "parse for getters");

    char *sv = cJSON_GetStringValue(cJSON_GetObjectItem(root, "name"));
    check(sv != NULL && strcmp(sv, "test") == 0, "get_string_value");

    double nv = cJSON_GetNumberValue(cJSON_GetObjectItem(root, "val"));
    check(nv > 3.13 && nv < 3.15, "get_number_value");

    cJSON_Delete(root);
}

/* ---- 4. Creation API ---- */
static void test_creation(void) {
    cJSON *root = cJSON_CreateObject();
    cJSON_AddStringToObject(root, "tool", "Noricum");
    cJSON_AddNumberToObject(root, "score", 100);
    cJSON_AddBoolToObject(root, "safe", 1);
    cJSON_AddNullToObject(root, "unsafe_blocks");

    cJSON *tags = cJSON_CreateArray();
    cJSON_AddItemToArray(tags, cJSON_CreateString("c2rust"));
    cJSON_AddItemToArray(tags, cJSON_CreateString("llm"));
    cJSON_AddItemToArray(tags, cJSON_CreateNumber(42));
    cJSON_AddItemToObject(root, "tags", tags);

    char *out = cJSON_PrintUnformatted(root);
    check(out != NULL, "create complex object");
    printf("Created: %s\n", out);

    /* Verify contents */
    check(cJSON_GetArraySize(tags) == 3, "array_size == 3");
    check(strcmp(cJSON_GetArrayItem(tags, 0)->valuestring, "c2rust") == 0, "array[0] == c2rust");
    check(cJSON_GetArrayItem(tags, 2)->valuedouble == 42.0, "array[2] == 42");

    cJSON_free(out);
    cJSON_Delete(root);
}

/* ---- 5. Array creators ---- */
static void test_array_creators(void) {
    int ints[] = {10, 20, 30, 40, 50};
    double doubles[] = {1.1, 2.2, 3.3};

    cJSON *ia = cJSON_CreateIntArray(ints, 5);
    check(cJSON_GetArraySize(ia) == 5, "int_array size 5");
    check(cJSON_GetArrayItem(ia, 2)->valueint == 30, "int_array[2] == 30");
    char *ia_str = cJSON_PrintUnformatted(ia);
    printf("IntArray: %s\n", ia_str);
    cJSON_free(ia_str);
    cJSON_Delete(ia);

    cJSON *da = cJSON_CreateDoubleArray(doubles, 3);
    check(cJSON_GetArraySize(da) == 3, "double_array size 3");
    char *da_str = cJSON_PrintUnformatted(da);
    printf("DoubleArray: %s\n", da_str);
    cJSON_free(da_str);
    cJSON_Delete(da);
}

/* ---- 6. Tree manipulation ---- */
static void test_manipulation(void) {
    cJSON *root = cJSON_Parse("{\"a\":1,\"b\":2,\"c\":3}");
    check(root != NULL, "parse for manipulation");

    /* Delete item */
    cJSON_DeleteItemFromObject(root, "b");
    check(cJSON_GetObjectItem(root, "b") == NULL, "delete b");

    /* Add new item */
    cJSON_AddStringToObject(root, "d", "new");
    check(cJSON_GetObjectItem(root, "d") != NULL, "add d");

    /* Replace item */
    cJSON_ReplaceItemInObject(root, "a", cJSON_CreateNumber(99));
    check(cJSON_GetObjectItem(root, "a")->valuedouble == 99.0, "replace a=99");

    /* Has item */
    check(cJSON_HasObjectItem(root, "c") == 1, "has c");
    check(cJSON_HasObjectItem(root, "b") == 0, "no b");

    char *out = cJSON_PrintUnformatted(root);
    printf("Manipulated: %s\n", out);
    cJSON_free(out);

    cJSON_Delete(root);
}

/* ---- 7. Array manipulation ---- */
static void test_array_manipulation(void) {
    cJSON *arr = cJSON_CreateArray();
    cJSON_AddItemToArray(arr, cJSON_CreateNumber(1));
    cJSON_AddItemToArray(arr, cJSON_CreateNumber(2));
    cJSON_AddItemToArray(arr, cJSON_CreateNumber(3));
    cJSON_AddItemToArray(arr, cJSON_CreateNumber(4));

    /* Insert at position */
    cJSON_InsertItemInArray(arr, 1, cJSON_CreateNumber(99));
    check(cJSON_GetArraySize(arr) == 5, "insert grows array");
    check(cJSON_GetArrayItem(arr, 1)->valuedouble == 99.0, "inserted at [1]");

    /* Delete from array */
    cJSON_DeleteItemFromArray(arr, 0);
    check(cJSON_GetArraySize(arr) == 4, "delete shrinks array");

    /* Detach */
    cJSON *detached = cJSON_DetachItemFromArray(arr, 0);
    check(detached != NULL && detached->valuedouble == 99.0, "detach returns item");
    cJSON_Delete(detached);

    char *out = cJSON_PrintUnformatted(arr);
    printf("Array: %s\n", out);
    cJSON_free(out);

    cJSON_Delete(arr);
}

/* ---- 8. Compare ---- */
static void test_compare(void) {
    cJSON *a = cJSON_Parse("{\"x\":1,\"y\":[2,3]}");
    cJSON *b = cJSON_Parse("{\"x\":1,\"y\":[2,3]}");
    cJSON *c = cJSON_Parse("{\"x\":1,\"y\":[2,4]}");

    check(cJSON_Compare(a, b, 1) == 1, "compare equal");
    check(cJSON_Compare(a, c, 1) == 0, "compare not equal");

    cJSON_Delete(a);
    cJSON_Delete(b);
    cJSON_Delete(c);
}

/* ---- 9. Duplicate ---- */
static void test_duplicate(void) {
    cJSON *orig = cJSON_Parse("{\"key\":\"value\",\"arr\":[1,2,3]}");
    cJSON *dup = cJSON_Duplicate(orig, 1);

    check(dup != NULL, "duplicate not null");
    check(cJSON_Compare(orig, dup, 1) == 1, "duplicate equals original");

    /* Modify duplicate shouldn't affect original */
    cJSON_ReplaceItemInObject(dup, "key", cJSON_CreateString("changed"));
    check(strcmp(cJSON_GetObjectItem(orig, "key")->valuestring, "value") == 0,
          "original unchanged after dup modify");

    char *out = cJSON_PrintUnformatted(dup);
    printf("Duplicate: %s\n", out);
    cJSON_free(out);

    cJSON_Delete(orig);
    cJSON_Delete(dup);
}

/* ---- 10. Minify ---- */
static void test_minify(void) {
    char json[] = "{\n  \"key\" : \"value\" ,\n  \"num\" : 42\n}";
    cJSON_Minify(json);
    check(strcmp(json, "{\"key\":\"value\",\"num\":42}") == 0, "minify");
    printf("Minified: %s\n", json);
}

/* ---- 11. Number edge cases ---- */
static void test_numbers(void) {
    cJSON *root = cJSON_Parse(
        "{\"zero\":0,\"neg\":-1,\"big\":1e10,\"small\":1e-10,"
        "\"max\":1.7976931348623157e308,\"pi\":3.14159265358979}");
    check(root != NULL, "parse numbers");

    check(cJSON_GetObjectItem(root, "zero")->valuedouble == 0.0, "zero");
    check(cJSON_GetObjectItem(root, "neg")->valuedouble == -1.0, "neg");
    check(cJSON_GetObjectItem(root, "pi")->valuedouble > 3.14, "pi");

    char *out = cJSON_PrintUnformatted(root);
    printf("Numbers: %s\n", out);
    cJSON_free(out);

    cJSON_Delete(root);
}

/* ---- 12. String escapes ---- */
static void test_string_escapes(void) {
    cJSON *root = cJSON_Parse(
        "{\"esc\":\"hello\\nworld\\ttab\\\"quote\\\\\\\\\"}");
    check(root != NULL, "parse escapes");

    const char *val = cJSON_GetObjectItem(root, "esc")->valuestring;
    check(strchr(val, '\n') != NULL, "contains newline");
    check(strchr(val, '\t') != NULL, "contains tab");

    char *out = cJSON_Print(root);
    printf("Escapes:\n%s\n", out);
    cJSON_free(out);

    cJSON_Delete(root);
}

/* ---- 13. Nested objects ---- */
static void test_nested(void) {
    const char *json =
        "{\"level1\":{\"level2\":{\"level3\":{\"value\":\"deep\"}}}}";
    cJSON *root = cJSON_Parse(json);
    check(root != NULL, "parse nested");

    cJSON *l1 = cJSON_GetObjectItem(root, "level1");
    cJSON *l2 = cJSON_GetObjectItem(l1, "level2");
    cJSON *l3 = cJSON_GetObjectItem(l2, "level3");
    cJSON *val = cJSON_GetObjectItem(l3, "value");
    check(val != NULL && strcmp(val->valuestring, "deep") == 0, "deep nested access");

    char *out = cJSON_PrintUnformatted(root);
    printf("Nested: %s\n", out);
    cJSON_free(out);

    cJSON_Delete(root);
}

/* ---- 14. Empty structures ---- */
static void test_empty(void) {
    cJSON *empty_obj = cJSON_Parse("{}");
    check(empty_obj != NULL && cJSON_IsObject(empty_obj), "parse empty object");
    char *eo = cJSON_PrintUnformatted(empty_obj);
    printf("EmptyObj: %s\n", eo);
    cJSON_free(eo);
    cJSON_Delete(empty_obj);

    cJSON *empty_arr = cJSON_Parse("[]");
    check(empty_arr != NULL && cJSON_IsArray(empty_arr), "parse empty array");
    char *ea = cJSON_PrintUnformatted(empty_arr);
    printf("EmptyArr: %s\n", ea);
    cJSON_free(ea);
    cJSON_Delete(empty_arr);
}

/* ---- 15. Parse errors ---- */
static void test_parse_errors(void) {
    cJSON *bad1 = cJSON_Parse("{invalid}");
    check(bad1 == NULL, "reject invalid json");

    cJSON *bad2 = cJSON_Parse("");
    check(bad2 == NULL, "reject empty string");

    cJSON *bad3 = cJSON_Parse(NULL);
    check(bad3 == NULL, "reject null input");
}

/* ---- 16. Case-sensitive object access ---- */
static void test_case_sensitive(void) {
    cJSON *root = cJSON_Parse("{\"Key\":1,\"key\":2,\"KEY\":3}");
    check(root != NULL, "parse case-sensitive keys");

    cJSON *k1 = cJSON_GetObjectItemCaseSensitive(root, "Key");
    cJSON *k2 = cJSON_GetObjectItemCaseSensitive(root, "key");
    cJSON *k3 = cJSON_GetObjectItemCaseSensitive(root, "KEY");

    check(k1 != NULL && k1->valueint == 1, "Key == 1");
    check(k2 != NULL && k2->valueint == 2, "key == 2");
    check(k3 != NULL && k3->valueint == 3, "KEY == 3");

    cJSON_Delete(root);
}

/* ---- main ---- */
int main(void) {
    printf("=== cJSON Full Migration Test ===\n\n");

    test_parse_print();
    test_type_checks();
    test_getters();
    test_creation();
    test_array_creators();
    test_manipulation();
    test_array_manipulation();
    test_compare();
    test_duplicate();
    test_minify();
    test_numbers();
    test_string_escapes();
    test_nested();
    test_empty();
    test_parse_errors();
    test_case_sensitive();

    printf("\n=== Results: %d/%d passed ===\n", pass_count, test_count);
    return (pass_count == test_count) ? 0 : 1;
}
