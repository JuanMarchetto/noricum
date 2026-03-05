/*
 * Minimal cJSON header — subset of the MIT-licensed cJSON library
 * by Dave Gamble (https://github.com/DaveGamble/cJSON).
 * This is a stripped-down version for Noricum test fixtures.
 */
#ifndef cJSON__h
#define cJSON__h

#ifdef __cplusplus
extern "C" {
#endif

/* cJSON Types */
#define cJSON_Invalid (0)
#define cJSON_False   (1 << 0)
#define cJSON_True    (1 << 1)
#define cJSON_NULL    (1 << 2)
#define cJSON_Number  (1 << 3)
#define cJSON_String  (1 << 4)
#define cJSON_Array   (1 << 5)
#define cJSON_Object  (1 << 6)
#define cJSON_Raw     (1 << 7)

/* The cJSON structure */
typedef struct cJSON {
    struct cJSON *next;
    struct cJSON *prev;
    struct cJSON *child;
    int type;
    char *valuestring;
    int valueint;
    double valuedouble;
    char *string;  /* key name */
} cJSON;

/* Creation */
cJSON *cJSON_CreateObject(void);
cJSON *cJSON_CreateString(const char *string);
cJSON *cJSON_CreateNumber(double num);
cJSON *cJSON_CreateBool(int boolean);
cJSON *cJSON_CreateIntArray(const int *numbers, int count);

/* Add to object */
cJSON *cJSON_AddStringToObject(cJSON *object, const char *name, const char *string);
cJSON *cJSON_AddNumberToObject(cJSON *object, const char *name, double number);
cJSON *cJSON_AddBoolToObject(cJSON *object, const char *name, int boolean);

/* Parsing */
cJSON *cJSON_Parse(const char *value);

/* Access */
cJSON *cJSON_GetObjectItem(const cJSON *object, const char *string);
cJSON *cJSON_GetArrayItem(const cJSON *array, int index);
int cJSON_GetArraySize(const cJSON *array);

/* Output */
char *cJSON_PrintUnformatted(const cJSON *item);

/* Cleanup */
void cJSON_Delete(cJSON *item);

#ifdef __cplusplus
}
#endif

#endif
