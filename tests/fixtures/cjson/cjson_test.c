#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "cJSON.h"

int main(void) {
    /* Test 1: Create and print object */
    cJSON *obj = cJSON_CreateObject();
    cJSON_AddStringToObject(obj, "name", "noricum");
    cJSON_AddNumberToObject(obj, "version", 1);
    cJSON_AddBoolToObject(obj, "valid", 1);
    char *json = cJSON_PrintUnformatted(obj);
    printf("create: %s\n", json);
    free(json);
    cJSON_Delete(obj);

    /* Test 2: Parse JSON */
    cJSON *parsed = cJSON_Parse("{\"key\":\"value\",\"num\":42}");
    printf("key=%s\n", cJSON_GetObjectItem(parsed, "key")->valuestring);
    printf("num=%d\n", cJSON_GetObjectItem(parsed, "num")->valueint);
    cJSON_Delete(parsed);

    /* Test 3: Array */
    int nums[] = {1, 2, 3, 4, 5};
    cJSON *arr = cJSON_CreateIntArray(nums, 5);
    printf("array_size=%d\n", cJSON_GetArraySize(arr));
    printf("item_2=%d\n", cJSON_GetArrayItem(arr, 2)->valueint);
    cJSON_Delete(arr);

    printf("done\n");
    return 0;
}
