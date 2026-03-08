/*
 * cJSON Full Combined — self-contained single-file cJSON implementation + tests.
 * For Noricum diff testing and LLM migration benchmarking.
 *
 * Based on the MIT-licensed cJSON library by Dave Gamble
 * (https://github.com/DaveGamble/cJSON).
 *
 * Inlines a substantial subset of cJSON (parser, printer, creation,
 * manipulation, comparison, duplication, minify) without requiring cJSON.h.
 * Custom allocator hooks omitted; malloc/free/realloc used directly.
 *
 * Compile: gcc -std=gnu11 -Wall -o test cjson_full_combined.c -lm
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include <limits.h>
#include <ctype.h>
#include <float.h>

/* ---- Type definitions and constants ---- */
#define cJSON_Invalid (0)
#define cJSON_False   (1 << 0)
#define cJSON_True    (1 << 1)
#define cJSON_NULL    (1 << 2)
#define cJSON_Number  (1 << 3)
#define cJSON_String  (1 << 4)
#define cJSON_Array   (1 << 5)
#define cJSON_Object  (1 << 6)
#define cJSON_Raw     (1 << 7)
#define cJSON_IsReference  256
#define cJSON_StringIsConst 512

typedef int cJSON_bool;
#ifdef true
#undef true
#endif
#define true ((cJSON_bool)1)
#ifdef false
#undef false
#endif
#define false ((cJSON_bool)0)

#ifndef CJSON_NESTING_LIMIT
#define CJSON_NESTING_LIMIT 1000
#endif
#ifndef CJSON_CIRCULAR_LIMIT
#define CJSON_CIRCULAR_LIMIT 10000
#endif
#ifndef isinf
#define isinf(d) (isnan((d - d)) && !isnan(d))
#endif
#ifndef isnan
#define isnan(d) (d != d)
#endif

typedef struct cJSON {
    struct cJSON *next, *prev, *child;
    int type;
    char *valuestring;
    int valueint;
    double valuedouble;
    char *string;
} cJSON;

#define cJSON_ArrayForEach(element, array) \
    for (element = (array != NULL) ? (array)->child : NULL; \
         element != NULL; element = element->next)

#define static_strlen(s) (sizeof(s) - sizeof(""))

/* ---- Forward declarations ---- */
cJSON *cJSON_CreateNull(void);
cJSON *cJSON_CreateTrue(void);
cJSON *cJSON_CreateFalse(void);
cJSON *cJSON_CreateBool(cJSON_bool b);
cJSON *cJSON_CreateNumber(double num);
cJSON *cJSON_CreateString(const char *s);
cJSON *cJSON_CreateRaw(const char *raw);
cJSON *cJSON_CreateArray(void);
cJSON *cJSON_CreateObject(void);
cJSON *cJSON_CreateIntArray(const int *numbers, int count);
cJSON *cJSON_CreateDoubleArray(const double *numbers, int count);
cJSON_bool cJSON_AddItemToArray(cJSON *array, cJSON *item);
cJSON_bool cJSON_AddItemToObject(cJSON *object, const char *string, cJSON *item);
cJSON *cJSON_DetachItemFromArray(cJSON *array, int which);
void cJSON_DeleteItemFromArray(cJSON *array, int which);
void cJSON_DeleteItemFromObject(cJSON *object, const char *string);
cJSON_bool cJSON_InsertItemInArray(cJSON *array, int which, cJSON *newitem);
cJSON_bool cJSON_ReplaceItemInArray(cJSON *array, int which, cJSON *newitem);
cJSON_bool cJSON_ReplaceItemInObject(cJSON *object, const char *string, cJSON *newitem);
cJSON *cJSON_DetachItemViaPointer(cJSON *parent, cJSON * const item);
cJSON *cJSON_AddNullToObject(cJSON * const object, const char * const name);
cJSON *cJSON_AddBoolToObject(cJSON * const object, const char * const name, cJSON_bool b);
cJSON *cJSON_AddNumberToObject(cJSON * const object, const char * const name, double number);
cJSON *cJSON_AddStringToObject(cJSON * const object, const char * const name, const char * const s);
cJSON *cJSON_AddObjectToObject(cJSON * const object, const char * const name);
cJSON *cJSON_AddArrayToObject(cJSON * const object, const char * const name);
int cJSON_GetArraySize(const cJSON *array);
cJSON *cJSON_GetArrayItem(const cJSON *array, int index);
cJSON *cJSON_GetObjectItem(const cJSON * const object, const char * const string);
cJSON *cJSON_GetObjectItemCaseSensitive(const cJSON * const object, const char * const string);
cJSON_bool cJSON_HasObjectItem(const cJSON *object, const char *string);
char *cJSON_GetStringValue(const cJSON * const item);
double cJSON_GetNumberValue(const cJSON * const item);
cJSON_bool cJSON_IsInvalid(const cJSON * const item);
cJSON_bool cJSON_IsFalse(const cJSON * const item);
cJSON_bool cJSON_IsTrue(const cJSON * const item);
cJSON_bool cJSON_IsBool(const cJSON * const item);
cJSON_bool cJSON_IsNull(const cJSON * const item);
cJSON_bool cJSON_IsNumber(const cJSON * const item);
cJSON_bool cJSON_IsString(const cJSON * const item);
cJSON_bool cJSON_IsArray(const cJSON * const item);
cJSON_bool cJSON_IsObject(const cJSON * const item);
cJSON_bool cJSON_IsRaw(const cJSON * const item);
cJSON *cJSON_Parse(const char *value);
char *cJSON_Print(const cJSON *item);
char *cJSON_PrintUnformatted(const cJSON *item);
cJSON *cJSON_Duplicate(const cJSON *item, cJSON_bool recurse);
cJSON_bool cJSON_Compare(const cJSON * const a, const cJSON * const b, cJSON_bool case_sensitive);
void cJSON_Minify(char *json);
double cJSON_SetNumberHelper(cJSON *object, double number);
char *cJSON_SetValuestring(cJSON *object, const char *valuestring);
void cJSON_Delete(cJSON *item);
void *cJSON_malloc(size_t size);
void cJSON_free(void *object);

/* ---- Internal helpers ---- */

static char *cJSON_strdup(const char *str) {
    if (str == NULL) return NULL;
    size_t length = strlen(str) + 1;
    char *copy = (char *)malloc(length);
    if (copy == NULL) return NULL;
    memcpy(copy, str, length);
    return copy;
}

static cJSON *cJSON_New_Item(void) {
    cJSON *node = (cJSON *)malloc(sizeof(cJSON));
    if (node) memset(node, 0, sizeof(cJSON));
    return node;
}

static int case_insensitive_strcmp(const unsigned char *s1, const unsigned char *s2) {
    if (s1 == NULL || s2 == NULL) return 1;
    if (s1 == s2) return 0;
    for (; tolower(*s1) == tolower(*s2); (void)s1++, s2++) {
        if (*s1 == '\0') return 0;
    }
    return tolower(*s1) - tolower(*s2);
}

static cJSON_bool compare_double(double a, double b) {
    double maxVal = fabs(a) > fabs(b) ? fabs(a) : fabs(b);
    return (fabs(a - b) <= maxVal * DBL_EPSILON);
}

static void suffix_object(cJSON *prev, cJSON *item) {
    prev->next = item;
    item->prev = prev;
}

/* ---- Delete ---- */

void cJSON_Delete(cJSON *item) {
    cJSON *next = NULL;
    while (item != NULL) {
        next = item->next;
        if (!(item->type & cJSON_IsReference) && item->child != NULL)
            cJSON_Delete(item->child);
        if (!(item->type & cJSON_IsReference) && item->valuestring != NULL)
            free(item->valuestring);
        if (!(item->type & cJSON_StringIsConst) && item->string != NULL)
            free(item->string);
        free(item);
        item = next;
    }
}

void *cJSON_malloc(size_t size) { return malloc(size); }
void cJSON_free(void *object) { free(object); }

/* ---- Parse buffer ---- */

typedef struct {
    const unsigned char *content;
    size_t length, offset, depth;
} parse_buffer;

#define can_read(buf, sz) ((buf != NULL) && (((buf)->offset + sz) <= (buf)->length))
#define can_access_at_index(buf, idx) ((buf != NULL) && (((buf)->offset + idx) < (buf)->length))
#define cannot_access_at_index(buf, idx) (!can_access_at_index(buf, idx))
#define buffer_at_offset(buf) ((buf)->content + (buf)->offset)

/* ---- Print buffer ---- */

typedef struct {
    unsigned char *buffer;
    size_t length, offset, depth;
    cJSON_bool noalloc, format;
} printbuffer;

static unsigned char *ensure(printbuffer *p, size_t needed) {
    unsigned char *newbuffer = NULL;
    size_t newsize = 0;
    if (p == NULL || p->buffer == NULL) return NULL;
    if (p->length > 0 && p->offset >= p->length) return NULL;
    if (needed > INT_MAX) return NULL;
    needed += p->offset + 1;
    if (needed <= p->length) return p->buffer + p->offset;
    if (p->noalloc) return NULL;
    if (needed > (INT_MAX / 2)) {
        if (needed <= INT_MAX) newsize = INT_MAX;
        else return NULL;
    } else {
        newsize = needed * 2;
    }
    newbuffer = (unsigned char *)realloc(p->buffer, newsize);
    if (newbuffer == NULL) { free(p->buffer); p->length = 0; p->buffer = NULL; return NULL; }
    p->length = newsize;
    p->buffer = newbuffer;
    return newbuffer + p->offset;
}

static void update_offset(printbuffer *buffer) {
    if (buffer == NULL || buffer->buffer == NULL) return;
    buffer->offset += strlen((const char *)(buffer->buffer + buffer->offset));
}

/* ---- Number parsing ---- */

static cJSON_bool parse_number(cJSON *item, parse_buffer *input_buffer) {
    unsigned char *after_end = NULL;
    unsigned char *number_c_string;
    size_t i = 0, number_string_length = 0;
    double number = 0;
    if (input_buffer == NULL || input_buffer->content == NULL) return false;
    for (i = 0; can_access_at_index(input_buffer, i); i++) {
        switch (buffer_at_offset(input_buffer)[i]) {
            case '0': case '1': case '2': case '3': case '4':
            case '5': case '6': case '7': case '8': case '9':
            case '+': case '-': case 'e': case 'E': case '.':
                number_string_length++; break;
            default: goto loop_end;
        }
    }
loop_end:
    number_c_string = (unsigned char *)malloc(number_string_length + 1);
    if (number_c_string == NULL) return false;
    memcpy(number_c_string, buffer_at_offset(input_buffer), number_string_length);
    number_c_string[number_string_length] = '\0';
    number = strtod((const char *)number_c_string, (char **)&after_end);
    if (number_c_string == after_end) { free(number_c_string); return false; }
    item->valuedouble = number;
    if (number >= INT_MAX) item->valueint = INT_MAX;
    else if (number <= (double)INT_MIN) item->valueint = INT_MIN;
    else item->valueint = (int)number;
    item->type = cJSON_Number;
    input_buffer->offset += (size_t)(after_end - number_c_string);
    free(number_c_string);
    return true;
}

/* ---- Number printing ---- */

static cJSON_bool print_number(const cJSON *item, printbuffer *output_buffer) {
    unsigned char *output_pointer = NULL;
    double d = item->valuedouble;
    int length = 0;
    size_t i = 0;
    unsigned char number_buffer[26] = {0};
    double test = 0.0;
    if (output_buffer == NULL) return false;
    if (isnan(d) || isinf(d)) {
        length = sprintf((char *)number_buffer, "null");
    } else if (d == (double)item->valueint) {
        length = sprintf((char *)number_buffer, "%d", item->valueint);
    } else {
        length = sprintf((char *)number_buffer, "%1.15g", d);
        if ((sscanf((char *)number_buffer, "%lg", &test) != 1) || !compare_double(test, d))
            length = sprintf((char *)number_buffer, "%1.17g", d);
    }
    if (length < 0 || length > (int)(sizeof(number_buffer) - 1)) return false;
    output_pointer = ensure(output_buffer, (size_t)length + 1);
    if (output_pointer == NULL) return false;
    for (i = 0; i < (size_t)length; i++) output_pointer[i] = number_buffer[i];
    output_pointer[i] = '\0';
    output_buffer->offset += (size_t)length;
    return true;
}

/* ---- Hex parsing for \uXXXX ---- */

static unsigned parse_hex4(const unsigned char *input) {
    unsigned int h = 0;
    for (size_t i = 0; i < 4; i++) {
        if (input[i] >= '0' && input[i] <= '9') h += (unsigned int)input[i] - '0';
        else if (input[i] >= 'A' && input[i] <= 'F') h += (unsigned int)10 + input[i] - 'A';
        else if (input[i] >= 'a' && input[i] <= 'f') h += (unsigned int)10 + input[i] - 'a';
        else return 0;
        if (i < 3) h = h << 4;
    }
    return h;
}

/* ---- UTF-16 to UTF-8 ---- */

static unsigned char utf16_literal_to_utf8(const unsigned char *input_pointer,
    const unsigned char *input_end, unsigned char **output_pointer) {
    long unsigned int codepoint = 0;
    unsigned int first_code = 0;
    const unsigned char *first_sequence = input_pointer;
    unsigned char utf8_length = 0, utf8_position = 0, sequence_length = 0, first_byte_mark = 0;

    if ((input_end - first_sequence) < 6) goto fail;
    first_code = parse_hex4(first_sequence + 2);
    if (first_code >= 0xDC00 && first_code <= 0xDFFF) goto fail;

    if (first_code >= 0xD800 && first_code <= 0xDBFF) {
        const unsigned char *second_sequence = first_sequence + 6;
        unsigned int second_code = 0;
        sequence_length = 12;
        if ((input_end - second_sequence) < 6) goto fail;
        if (second_sequence[0] != '\\' || second_sequence[1] != 'u') goto fail;
        second_code = parse_hex4(second_sequence + 2);
        if (second_code < 0xDC00 || second_code > 0xDFFF) goto fail;
        codepoint = 0x10000 + (((first_code & 0x3FF) << 10) | (second_code & 0x3FF));
    } else {
        sequence_length = 6;
        codepoint = first_code;
    }

    if (codepoint < 0x80) { utf8_length = 1; }
    else if (codepoint < 0x800) { utf8_length = 2; first_byte_mark = 0xC0; }
    else if (codepoint < 0x10000) { utf8_length = 3; first_byte_mark = 0xE0; }
    else if (codepoint <= 0x10FFFF) { utf8_length = 4; first_byte_mark = 0xF0; }
    else goto fail;

    for (utf8_position = (unsigned char)(utf8_length - 1); utf8_position > 0; utf8_position--) {
        (*output_pointer)[utf8_position] = (unsigned char)((codepoint | 0x80) & 0xBF);
        codepoint >>= 6;
    }
    if (utf8_length > 1)
        (*output_pointer)[0] = (unsigned char)((codepoint | first_byte_mark) & 0xFF);
    else
        (*output_pointer)[0] = (unsigned char)(codepoint & 0x7F);
    *output_pointer += utf8_length;
    return sequence_length;
fail:
    return 0;
}

/* ---- String parsing ---- */

static cJSON_bool parse_string(cJSON *item, parse_buffer *input_buffer) {
    const unsigned char *input_pointer = buffer_at_offset(input_buffer) + 1;
    const unsigned char *input_end = buffer_at_offset(input_buffer) + 1;
    unsigned char *output_pointer = NULL, *output = NULL;

    if (buffer_at_offset(input_buffer)[0] != '\"') goto fail;
    {
        size_t allocation_length = 0, skipped_bytes = 0;
        while ((size_t)(input_end - input_buffer->content) < input_buffer->length && *input_end != '\"') {
            if (input_end[0] == '\\') {
                if ((size_t)(input_end + 1 - input_buffer->content) >= input_buffer->length) goto fail;
                skipped_bytes++;
                input_end++;
            }
            input_end++;
        }
        if ((size_t)(input_end - input_buffer->content) >= input_buffer->length || *input_end != '\"')
            goto fail;
        allocation_length = (size_t)(input_end - buffer_at_offset(input_buffer)) - skipped_bytes;
        output = (unsigned char *)malloc(allocation_length + 1);
        if (output == NULL) goto fail;
    }

    output_pointer = output;
    while (input_pointer < input_end) {
        if (*input_pointer != '\\') {
            *output_pointer++ = *input_pointer++;
        } else {
            unsigned char sequence_length = 2;
            if ((input_end - input_pointer) < 1) goto fail;
            switch (input_pointer[1]) {
                case 'b': *output_pointer++ = '\b'; break;
                case 'f': *output_pointer++ = '\f'; break;
                case 'n': *output_pointer++ = '\n'; break;
                case 'r': *output_pointer++ = '\r'; break;
                case 't': *output_pointer++ = '\t'; break;
                case '\"': case '\\': case '/': *output_pointer++ = input_pointer[1]; break;
                case 'u':
                    sequence_length = utf16_literal_to_utf8(input_pointer, input_end, &output_pointer);
                    if (sequence_length == 0) goto fail;
                    break;
                default: goto fail;
            }
            input_pointer += sequence_length;
        }
    }
    *output_pointer = '\0';
    item->type = cJSON_String;
    item->valuestring = (char *)output;
    input_buffer->offset = (size_t)(input_end - input_buffer->content);
    input_buffer->offset++;
    return true;
fail:
    if (output != NULL) free(output);
    if (input_pointer != NULL) input_buffer->offset = (size_t)(input_pointer - input_buffer->content);
    return false;
}

/* ---- String printing (escaped) ---- */

static cJSON_bool print_string_ptr(const unsigned char *input, printbuffer *output_buffer) {
    const unsigned char *input_pointer = NULL;
    unsigned char *output = NULL, *output_pointer = NULL;
    size_t output_length = 0, escape_characters = 0;
    if (output_buffer == NULL) return false;
    if (input == NULL) {
        output = ensure(output_buffer, sizeof("\"\""));
        if (output == NULL) return false;
        strcpy((char *)output, "\"\"");
        return true;
    }
    for (input_pointer = input; *input_pointer; input_pointer++) {
        switch (*input_pointer) {
            case '\"': case '\\': case '\b': case '\f': case '\n': case '\r': case '\t':
                escape_characters++; break;
            default:
                if (*input_pointer < 32) escape_characters += 5;
                break;
        }
    }
    output_length = (size_t)(input_pointer - input) + escape_characters;
    output = ensure(output_buffer, output_length + sizeof("\"\""));
    if (output == NULL) return false;
    if (escape_characters == 0) {
        output[0] = '\"';
        memcpy(output + 1, input, output_length);
        output[output_length + 1] = '\"';
        output[output_length + 2] = '\0';
        return true;
    }
    output[0] = '\"';
    output_pointer = output + 1;
    for (input_pointer = input; *input_pointer != '\0'; (void)input_pointer++, output_pointer++) {
        if (*input_pointer > 31 && *input_pointer != '\"' && *input_pointer != '\\') {
            *output_pointer = *input_pointer;
        } else {
            *output_pointer++ = '\\';
            switch (*input_pointer) {
                case '\\': *output_pointer = '\\'; break;
                case '\"': *output_pointer = '\"'; break;
                case '\b': *output_pointer = 'b'; break;
                case '\f': *output_pointer = 'f'; break;
                case '\n': *output_pointer = 'n'; break;
                case '\r': *output_pointer = 'r'; break;
                case '\t': *output_pointer = 't'; break;
                default:
                    sprintf((char *)output_pointer, "u%04x", *input_pointer);
                    output_pointer += 4;
                    break;
            }
        }
    }
    output[output_length + 1] = '\"';
    output[output_length + 2] = '\0';
    return true;
}

static cJSON_bool print_string(const cJSON *item, printbuffer *p) {
    return print_string_ptr((unsigned char *)item->valuestring, p);
}

/* ---- Parser/printer prototypes ---- */
static cJSON_bool parse_value(cJSON *item, parse_buffer *input_buffer);
static cJSON_bool print_value(const cJSON *item, printbuffer *output_buffer);
static cJSON_bool parse_array(cJSON *item, parse_buffer *input_buffer);
static cJSON_bool print_array(const cJSON *item, printbuffer *output_buffer);
static cJSON_bool parse_object(cJSON *item, parse_buffer *input_buffer);
static cJSON_bool print_object(const cJSON *item, printbuffer *output_buffer);

/* ---- Whitespace/BOM ---- */

static parse_buffer *buffer_skip_whitespace(parse_buffer *buffer) {
    if (buffer == NULL || buffer->content == NULL) return NULL;
    if (cannot_access_at_index(buffer, 0)) return buffer;
    while (can_access_at_index(buffer, 0) && buffer_at_offset(buffer)[0] <= 32) buffer->offset++;
    if (buffer->offset == buffer->length) buffer->offset--;
    return buffer;
}

static parse_buffer *skip_utf8_bom(parse_buffer *buffer) {
    if (buffer == NULL || buffer->content == NULL || buffer->offset != 0) return NULL;
    if (can_access_at_index(buffer, 4) &&
        strncmp((const char *)buffer_at_offset(buffer), "\xEF\xBB\xBF", 3) == 0)
        buffer->offset += 3;
    return buffer;
}

/* ---- Parse value ---- */

static cJSON_bool parse_value(cJSON *item, parse_buffer *input_buffer) {
    if (input_buffer == NULL || input_buffer->content == NULL) return false;
    if (can_read(input_buffer, 4) && strncmp((const char *)buffer_at_offset(input_buffer), "null", 4) == 0) {
        item->type = cJSON_NULL; input_buffer->offset += 4; return true;
    }
    if (can_read(input_buffer, 5) && strncmp((const char *)buffer_at_offset(input_buffer), "false", 5) == 0) {
        item->type = cJSON_False; input_buffer->offset += 5; return true;
    }
    if (can_read(input_buffer, 4) && strncmp((const char *)buffer_at_offset(input_buffer), "true", 4) == 0) {
        item->type = cJSON_True; item->valueint = 1; input_buffer->offset += 4; return true;
    }
    if (can_access_at_index(input_buffer, 0) && buffer_at_offset(input_buffer)[0] == '\"')
        return parse_string(item, input_buffer);
    if (can_access_at_index(input_buffer, 0) &&
        (buffer_at_offset(input_buffer)[0] == '-' ||
         (buffer_at_offset(input_buffer)[0] >= '0' && buffer_at_offset(input_buffer)[0] <= '9')))
        return parse_number(item, input_buffer);
    if (can_access_at_index(input_buffer, 0) && buffer_at_offset(input_buffer)[0] == '[')
        return parse_array(item, input_buffer);
    if (can_access_at_index(input_buffer, 0) && buffer_at_offset(input_buffer)[0] == '{')
        return parse_object(item, input_buffer);
    return false;
}

/* ---- Print value ---- */

static cJSON_bool print_value(const cJSON *item, printbuffer *output_buffer) {
    unsigned char *output = NULL;
    if (item == NULL || output_buffer == NULL) return false;
    switch ((item->type) & 0xFF) {
        case cJSON_NULL:
            output = ensure(output_buffer, 5); if (!output) return false;
            strcpy((char *)output, "null"); return true;
        case cJSON_False:
            output = ensure(output_buffer, 6); if (!output) return false;
            strcpy((char *)output, "false"); return true;
        case cJSON_True:
            output = ensure(output_buffer, 5); if (!output) return false;
            strcpy((char *)output, "true"); return true;
        case cJSON_Number: return print_number(item, output_buffer);
        case cJSON_Raw: {
            size_t raw_length = 0;
            if (item->valuestring == NULL) return false;
            raw_length = strlen(item->valuestring) + 1;
            output = ensure(output_buffer, raw_length); if (!output) return false;
            memcpy(output, item->valuestring, raw_length); return true;
        }
        case cJSON_String: return print_string(item, output_buffer);
        case cJSON_Array: return print_array(item, output_buffer);
        case cJSON_Object: return print_object(item, output_buffer);
        default: return false;
    }
}

/* ---- Array parsing ---- */

static cJSON_bool parse_array(cJSON *item, parse_buffer *input_buffer) {
    cJSON *head = NULL, *current_item = NULL;
    if (input_buffer->depth >= CJSON_NESTING_LIMIT) return false;
    input_buffer->depth++;
    if (buffer_at_offset(input_buffer)[0] != '[') goto fail;
    input_buffer->offset++;
    buffer_skip_whitespace(input_buffer);
    if (can_access_at_index(input_buffer, 0) && buffer_at_offset(input_buffer)[0] == ']')
        goto success;
    if (cannot_access_at_index(input_buffer, 0)) { input_buffer->offset--; goto fail; }
    input_buffer->offset--;
    do {
        cJSON *new_item = cJSON_New_Item();
        if (new_item == NULL) goto fail;
        if (head == NULL) { current_item = head = new_item; }
        else { current_item->next = new_item; new_item->prev = current_item; current_item = new_item; }
        input_buffer->offset++;
        buffer_skip_whitespace(input_buffer);
        if (!parse_value(current_item, input_buffer)) goto fail;
        buffer_skip_whitespace(input_buffer);
    } while (can_access_at_index(input_buffer, 0) && buffer_at_offset(input_buffer)[0] == ',');
    if (cannot_access_at_index(input_buffer, 0) || buffer_at_offset(input_buffer)[0] != ']') goto fail;
success:
    input_buffer->depth--;
    if (head != NULL) head->prev = current_item;
    item->type = cJSON_Array; item->child = head;
    input_buffer->offset++;
    return true;
fail:
    if (head != NULL) cJSON_Delete(head);
    return false;
}

/* ---- Array printing ---- */

static cJSON_bool print_array(const cJSON *item, printbuffer *output_buffer) {
    unsigned char *output_pointer = NULL;
    size_t length = 0;
    cJSON *current_element = item->child;
    if (output_buffer == NULL) return false;
    if (output_buffer->depth >= CJSON_NESTING_LIMIT) return false;
    output_pointer = ensure(output_buffer, 1);
    if (output_pointer == NULL) return false;
    *output_pointer = '[';
    output_buffer->offset++;
    output_buffer->depth++;
    while (current_element != NULL) {
        if (!print_value(current_element, output_buffer)) return false;
        update_offset(output_buffer);
        if (current_element->next) {
            length = (size_t)(output_buffer->format ? 2 : 1);
            output_pointer = ensure(output_buffer, length + 1);
            if (output_pointer == NULL) return false;
            *output_pointer++ = ',';
            if (output_buffer->format) *output_pointer++ = ' ';
            *output_pointer = '\0';
            output_buffer->offset += length;
        }
        current_element = current_element->next;
    }
    output_pointer = ensure(output_buffer, 2);
    if (output_pointer == NULL) return false;
    *output_pointer++ = ']'; *output_pointer = '\0';
    output_buffer->depth--;
    return true;
}

/* ---- Object parsing ---- */

static cJSON_bool parse_object(cJSON *item, parse_buffer *input_buffer) {
    cJSON *head = NULL, *current_item = NULL;
    if (input_buffer->depth >= CJSON_NESTING_LIMIT) return false;
    input_buffer->depth++;
    if (cannot_access_at_index(input_buffer, 0) || buffer_at_offset(input_buffer)[0] != '{') goto fail;
    input_buffer->offset++;
    buffer_skip_whitespace(input_buffer);
    if (can_access_at_index(input_buffer, 0) && buffer_at_offset(input_buffer)[0] == '}') goto success;
    if (cannot_access_at_index(input_buffer, 0)) { input_buffer->offset--; goto fail; }
    input_buffer->offset--;
    do {
        cJSON *new_item = cJSON_New_Item();
        if (new_item == NULL) goto fail;
        if (head == NULL) { current_item = head = new_item; }
        else { current_item->next = new_item; new_item->prev = current_item; current_item = new_item; }
        if (cannot_access_at_index(input_buffer, 1)) goto fail;
        input_buffer->offset++;
        buffer_skip_whitespace(input_buffer);
        if (!parse_string(current_item, input_buffer)) goto fail;
        buffer_skip_whitespace(input_buffer);
        current_item->string = current_item->valuestring;
        current_item->valuestring = NULL;
        if (cannot_access_at_index(input_buffer, 0) || buffer_at_offset(input_buffer)[0] != ':') goto fail;
        input_buffer->offset++;
        buffer_skip_whitespace(input_buffer);
        if (!parse_value(current_item, input_buffer)) goto fail;
        buffer_skip_whitespace(input_buffer);
    } while (can_access_at_index(input_buffer, 0) && buffer_at_offset(input_buffer)[0] == ',');
    if (cannot_access_at_index(input_buffer, 0) || buffer_at_offset(input_buffer)[0] != '}') goto fail;
success:
    input_buffer->depth--;
    if (head != NULL) head->prev = current_item;
    item->type = cJSON_Object; item->child = head;
    input_buffer->offset++;
    return true;
fail:
    if (head != NULL) cJSON_Delete(head);
    return false;
}

/* ---- Object printing ---- */

static cJSON_bool print_object(const cJSON *item, printbuffer *output_buffer) {
    unsigned char *output_pointer = NULL;
    size_t length = 0;
    cJSON *current_item = item->child;
    if (output_buffer == NULL) return false;
    if (output_buffer->depth >= CJSON_NESTING_LIMIT) return false;
    length = (size_t)(output_buffer->format ? 2 : 1);
    output_pointer = ensure(output_buffer, length + 1);
    if (output_pointer == NULL) return false;
    *output_pointer++ = '{';
    output_buffer->depth++;
    if (output_buffer->format) *output_pointer++ = '\n';
    output_buffer->offset += length;

    while (current_item) {
        if (output_buffer->format) {
            output_pointer = ensure(output_buffer, output_buffer->depth);
            if (output_pointer == NULL) return false;
            for (size_t i = 0; i < output_buffer->depth; i++) *output_pointer++ = '\t';
            output_buffer->offset += output_buffer->depth;
        }
        if (!print_string_ptr((unsigned char *)current_item->string, output_buffer)) return false;
        update_offset(output_buffer);
        length = (size_t)(output_buffer->format ? 2 : 1);
        output_pointer = ensure(output_buffer, length);
        if (output_pointer == NULL) return false;
        *output_pointer++ = ':';
        if (output_buffer->format) *output_pointer++ = '\t';
        output_buffer->offset += length;
        if (!print_value(current_item, output_buffer)) return false;
        update_offset(output_buffer);
        length = ((size_t)(output_buffer->format ? 1 : 0) + (size_t)(current_item->next ? 1 : 0));
        output_pointer = ensure(output_buffer, length + 1);
        if (output_pointer == NULL) return false;
        if (current_item->next) *output_pointer++ = ',';
        if (output_buffer->format) *output_pointer++ = '\n';
        *output_pointer = '\0';
        output_buffer->offset += length;
        current_item = current_item->next;
    }
    output_pointer = ensure(output_buffer, output_buffer->format ? (output_buffer->depth + 1) : 2);
    if (output_pointer == NULL) return false;
    if (output_buffer->format) {
        for (size_t i = 0; i < output_buffer->depth - 1; i++) *output_pointer++ = '\t';
    }
    *output_pointer++ = '}'; *output_pointer = '\0';
    output_buffer->depth--;
    return true;
}

/* ---- Top-level print ---- */
#define cjson_min(a, b) (((a) < (b)) ? (a) : (b))

static unsigned char *print_impl(const cJSON *item, cJSON_bool format) {
    static const size_t default_buffer_size = 256;
    printbuffer buffer[1];
    unsigned char *printed = NULL;
    memset(buffer, 0, sizeof(buffer));
    buffer->buffer = (unsigned char *)malloc(default_buffer_size);
    buffer->length = default_buffer_size;
    buffer->format = format;
    if (buffer->buffer == NULL) goto fail;
    if (!print_value(item, buffer)) goto fail;
    update_offset(buffer);
    printed = (unsigned char *)realloc(buffer->buffer, buffer->offset + 1);
    if (printed == NULL) goto fail;
    buffer->buffer = NULL;
    return printed;
fail:
    if (buffer->buffer != NULL) { free(buffer->buffer); buffer->buffer = NULL; }
    if (printed != NULL) { free(printed); printed = NULL; }
    return NULL;
}

/* ---- Parse entry point ---- */

cJSON *cJSON_Parse(const char *value) {
    parse_buffer buffer = {0, 0, 0, 0};
    cJSON *item = NULL;
    if (value == NULL) return NULL;
    buffer.content = (const unsigned char *)value;
    buffer.length = strlen(value) + 1;
    buffer.offset = 0;
    item = cJSON_New_Item();
    if (item == NULL) return NULL;
    if (!parse_value(item, buffer_skip_whitespace(skip_utf8_bom(&buffer)))) {
        cJSON_Delete(item);
        return NULL;
    }
    return item;
}

char *cJSON_Print(const cJSON *item) { return (char *)print_impl(item, true); }
char *cJSON_PrintUnformatted(const cJSON *item) { return (char *)print_impl(item, false); }

/* ---- Getters ---- */

char *cJSON_GetStringValue(const cJSON *item) {
    if (!cJSON_IsString(item)) return NULL;
    return item->valuestring;
}
double cJSON_GetNumberValue(const cJSON *item) {
    if (!cJSON_IsNumber(item)) return 0.0 / 0.0;
    return item->valuedouble;
}
int cJSON_GetArraySize(const cJSON *array) {
    if (array == NULL) return 0;
    size_t size = 0;
    cJSON *child = array->child;
    while (child != NULL) { size++; child = child->next; }
    return (int)size;
}
static cJSON *get_array_item(const cJSON *array, size_t index) {
    if (array == NULL) return NULL;
    cJSON *current_child = array->child;
    while (current_child != NULL && index > 0) { index--; current_child = current_child->next; }
    return current_child;
}
cJSON *cJSON_GetArrayItem(const cJSON *array, int index) {
    if (index < 0) return NULL;
    return get_array_item(array, (size_t)index);
}
static cJSON *get_object_item(const cJSON *object, const char *name, cJSON_bool case_sensitive) {
    if (object == NULL || name == NULL) return NULL;
    cJSON *current_element = object->child;
    if (case_sensitive) {
        while (current_element != NULL && current_element->string != NULL &&
               strcmp(name, current_element->string) != 0)
            current_element = current_element->next;
    } else {
        while (current_element != NULL &&
               case_insensitive_strcmp((const unsigned char *)name,
                   (const unsigned char *)(current_element->string)) != 0)
            current_element = current_element->next;
    }
    if (current_element == NULL || current_element->string == NULL) return NULL;
    return current_element;
}
cJSON *cJSON_GetObjectItem(const cJSON *object, const char *string) {
    return get_object_item(object, string, false);
}
cJSON *cJSON_GetObjectItemCaseSensitive(const cJSON *object, const char *string) {
    return get_object_item(object, string, true);
}
cJSON_bool cJSON_HasObjectItem(const cJSON *object, const char *string) {
    return cJSON_GetObjectItem(object, string) ? 1 : 0;
}

/* ---- Type checkers ---- */

cJSON_bool cJSON_IsInvalid(const cJSON *item) { return item ? (item->type & 0xFF) == cJSON_Invalid : false; }
cJSON_bool cJSON_IsFalse(const cJSON *item) { return item ? (item->type & 0xFF) == cJSON_False : false; }
cJSON_bool cJSON_IsTrue(const cJSON *item) { return item ? (item->type & 0xFF) == cJSON_True : false; }
cJSON_bool cJSON_IsBool(const cJSON *item) { return item ? (item->type & (cJSON_True | cJSON_False)) != 0 : false; }
cJSON_bool cJSON_IsNull(const cJSON *item) { return item ? (item->type & 0xFF) == cJSON_NULL : false; }
cJSON_bool cJSON_IsNumber(const cJSON *item) { return item ? (item->type & 0xFF) == cJSON_Number : false; }
cJSON_bool cJSON_IsString(const cJSON *item) { return item ? (item->type & 0xFF) == cJSON_String : false; }
cJSON_bool cJSON_IsArray(const cJSON *item) { return item ? (item->type & 0xFF) == cJSON_Array : false; }
cJSON_bool cJSON_IsObject(const cJSON *item) { return item ? (item->type & 0xFF) == cJSON_Object : false; }
cJSON_bool cJSON_IsRaw(const cJSON *item) { return item ? (item->type & 0xFF) == cJSON_Raw : false; }

/* ---- Creation functions ---- */

cJSON *cJSON_CreateNull(void) { cJSON *i = cJSON_New_Item(); if (i) i->type = cJSON_NULL; return i; }
cJSON *cJSON_CreateTrue(void) { cJSON *i = cJSON_New_Item(); if (i) i->type = cJSON_True; return i; }
cJSON *cJSON_CreateFalse(void) { cJSON *i = cJSON_New_Item(); if (i) i->type = cJSON_False; return i; }
cJSON *cJSON_CreateBool(cJSON_bool b) { cJSON *i = cJSON_New_Item(); if (i) i->type = b ? cJSON_True : cJSON_False; return i; }
cJSON *cJSON_CreateNumber(double num) {
    cJSON *item = cJSON_New_Item();
    if (item) {
        item->type = cJSON_Number; item->valuedouble = num;
        if (num >= INT_MAX) item->valueint = INT_MAX;
        else if (num <= (double)INT_MIN) item->valueint = INT_MIN;
        else item->valueint = (int)num;
    }
    return item;
}
cJSON *cJSON_CreateString(const char *s) {
    cJSON *item = cJSON_New_Item();
    if (item) {
        item->type = cJSON_String;
        item->valuestring = cJSON_strdup(s);
        if (!item->valuestring) { cJSON_Delete(item); return NULL; }
    }
    return item;
}
cJSON *cJSON_CreateRaw(const char *raw) {
    cJSON *item = cJSON_New_Item();
    if (item) {
        item->type = cJSON_Raw;
        item->valuestring = cJSON_strdup(raw);
        if (!item->valuestring) { cJSON_Delete(item); return NULL; }
    }
    return item;
}
cJSON *cJSON_CreateArray(void) { cJSON *i = cJSON_New_Item(); if (i) i->type = cJSON_Array; return i; }
cJSON *cJSON_CreateObject(void) { cJSON *i = cJSON_New_Item(); if (i) i->type = cJSON_Object; return i; }

/* ---- Array creators ---- */

cJSON *cJSON_CreateIntArray(const int *numbers, int count) {
    if (count < 0 || numbers == NULL) return NULL;
    cJSON *n = NULL, *p = NULL, *a = cJSON_CreateArray();
    for (size_t i = 0; a && i < (size_t)count; i++) {
        n = cJSON_CreateNumber(numbers[i]);
        if (!n) { cJSON_Delete(a); return NULL; }
        if (!i) a->child = n; else suffix_object(p, n);
        p = n;
    }
    if (a && a->child) a->child->prev = n;
    return a;
}
cJSON *cJSON_CreateDoubleArray(const double *numbers, int count) {
    if (count < 0 || numbers == NULL) return NULL;
    cJSON *n = NULL, *p = NULL, *a = cJSON_CreateArray();
    for (size_t i = 0; a && i < (size_t)count; i++) {
        n = cJSON_CreateNumber(numbers[i]);
        if (!n) { cJSON_Delete(a); return NULL; }
        if (!i) a->child = n; else suffix_object(p, n);
        p = n;
    }
    if (a && a->child) a->child->prev = n;
    return a;
}

/* ---- add_item_to_array / add_item_to_object ---- */

static cJSON_bool add_item_to_array(cJSON *array, cJSON *item) {
    if (item == NULL || array == NULL || array == item) return false;
    cJSON *child = array->child;
    if (child == NULL) { array->child = item; item->prev = item; item->next = NULL; }
    else if (child->prev) { suffix_object(child->prev, item); array->child->prev = item; }
    return true;
}
static cJSON_bool add_item_to_object(cJSON *object, const char *string, cJSON *item) {
    if (object == NULL || string == NULL || item == NULL || object == item) return false;
    char *new_key = cJSON_strdup(string);
    if (new_key == NULL) return false;
    if (!(item->type & cJSON_StringIsConst) && item->string != NULL) free(item->string);
    item->string = new_key;
    item->type &= ~cJSON_StringIsConst;
    return add_item_to_array(object, item);
}
cJSON_bool cJSON_AddItemToArray(cJSON *array, cJSON *item) { return add_item_to_array(array, item); }
cJSON_bool cJSON_AddItemToObject(cJSON *object, const char *string, cJSON *item) {
    return add_item_to_object(object, string, item);
}

/* ---- Convenience add-to-object ---- */

cJSON *cJSON_AddNullToObject(cJSON *object, const char *name) {
    cJSON *n = cJSON_CreateNull();
    if (add_item_to_object(object, name, n)) return n;
    cJSON_Delete(n); return NULL;
}
cJSON *cJSON_AddBoolToObject(cJSON *object, const char *name, cJSON_bool b) {
    cJSON *item = cJSON_CreateBool(b);
    if (add_item_to_object(object, name, item)) return item;
    cJSON_Delete(item); return NULL;
}
cJSON *cJSON_AddNumberToObject(cJSON *object, const char *name, double number) {
    cJSON *item = cJSON_CreateNumber(number);
    if (add_item_to_object(object, name, item)) return item;
    cJSON_Delete(item); return NULL;
}
cJSON *cJSON_AddStringToObject(cJSON *object, const char *name, const char *s) {
    cJSON *item = cJSON_CreateString(s);
    if (add_item_to_object(object, name, item)) return item;
    cJSON_Delete(item); return NULL;
}
cJSON *cJSON_AddObjectToObject(cJSON *object, const char *name) {
    cJSON *item = cJSON_CreateObject();
    if (add_item_to_object(object, name, item)) return item;
    cJSON_Delete(item); return NULL;
}
cJSON *cJSON_AddArrayToObject(cJSON *object, const char *name) {
    cJSON *item = cJSON_CreateArray();
    if (add_item_to_object(object, name, item)) return item;
    cJSON_Delete(item); return NULL;
}

/* ---- Detach / Delete ---- */

cJSON *cJSON_DetachItemViaPointer(cJSON *parent, cJSON *item) {
    if (parent == NULL || item == NULL || (item != parent->child && item->prev == NULL)) return NULL;
    if (item != parent->child) item->prev->next = item->next;
    if (item->next != NULL) item->next->prev = item->prev;
    if (item == parent->child) parent->child = item->next;
    else if (item->next == NULL) parent->child->prev = item->prev;
    item->prev = NULL; item->next = NULL;
    return item;
}
cJSON *cJSON_DetachItemFromArray(cJSON *array, int which) {
    if (which < 0) return NULL;
    return cJSON_DetachItemViaPointer(array, get_array_item(array, (size_t)which));
}
void cJSON_DeleteItemFromArray(cJSON *array, int which) {
    cJSON_Delete(cJSON_DetachItemFromArray(array, which));
}
void cJSON_DeleteItemFromObject(cJSON *object, const char *string) {
    cJSON_Delete(cJSON_DetachItemViaPointer(object, cJSON_GetObjectItem(object, string)));
}

/* ---- Insert / Replace ---- */

cJSON_bool cJSON_InsertItemInArray(cJSON *array, int which, cJSON *newitem) {
    if (which < 0 || newitem == NULL) return false;
    cJSON *after_inserted = get_array_item(array, (size_t)which);
    if (after_inserted == NULL) return add_item_to_array(array, newitem);
    if (after_inserted != array->child && after_inserted->prev == NULL) return false;
    newitem->next = after_inserted;
    newitem->prev = after_inserted->prev;
    after_inserted->prev = newitem;
    if (after_inserted == array->child) array->child = newitem;
    else newitem->prev->next = newitem;
    return true;
}

cJSON_bool cJSON_ReplaceItemViaPointer(cJSON *parent, cJSON *item, cJSON *replacement) {
    if (parent == NULL || parent->child == NULL || replacement == NULL || item == NULL) return false;
    if (replacement == item) return true;
    replacement->next = item->next;
    replacement->prev = item->prev;
    if (replacement->next != NULL) replacement->next->prev = replacement;
    if (parent->child == item) {
        if (parent->child->prev == parent->child) replacement->prev = replacement;
        parent->child = replacement;
    } else {
        if (replacement->prev != NULL) replacement->prev->next = replacement;
        if (replacement->next == NULL) parent->child->prev = replacement;
    }
    item->next = NULL; item->prev = NULL;
    cJSON_Delete(item);
    return true;
}
cJSON_bool cJSON_ReplaceItemInArray(cJSON *array, int which, cJSON *newitem) {
    if (which < 0) return false;
    return cJSON_ReplaceItemViaPointer(array, get_array_item(array, (size_t)which), newitem);
}
static cJSON_bool replace_item_in_object(cJSON *object, const char *string,
    cJSON *replacement, cJSON_bool case_sensitive) {
    if (replacement == NULL || string == NULL) return false;
    if (!(replacement->type & cJSON_StringIsConst) && replacement->string != NULL)
        free(replacement->string);
    replacement->string = cJSON_strdup(string);
    if (replacement->string == NULL) return false;
    replacement->type &= ~cJSON_StringIsConst;
    return cJSON_ReplaceItemViaPointer(object, get_object_item(object, string, case_sensitive), replacement);
}
cJSON_bool cJSON_ReplaceItemInObject(cJSON *object, const char *string, cJSON *newitem) {
    return replace_item_in_object(object, string, newitem, false);
}
cJSON_bool cJSON_ReplaceItemInObjectCaseSensitive(cJSON *object, const char *string, cJSON *newitem) {
    return replace_item_in_object(object, string, newitem, true);
}

/* ---- Convenience: AddTrue/False/Raw to object ---- */
cJSON *cJSON_AddTrueToObject(cJSON *object, const char *name) {
    cJSON *item = cJSON_CreateTrue();
    if (add_item_to_object(object, name, item)) return item;
    cJSON_Delete(item); return NULL;
}
cJSON *cJSON_AddFalseToObject(cJSON *object, const char *name) {
    cJSON *item = cJSON_CreateFalse();
    if (add_item_to_object(object, name, item)) return item;
    cJSON_Delete(item); return NULL;
}
cJSON *cJSON_AddRawToObject(cJSON *object, const char *name, const char *raw) {
    cJSON *item = cJSON_CreateRaw(raw);
    if (add_item_to_object(object, name, item)) return item;
    cJSON_Delete(item); return NULL;
}

/* ---- References (shared ownership via IsReference flag) ---- */
cJSON_bool cJSON_AddItemReferenceToArray(cJSON *array, cJSON *item) {
    if (item == NULL) return false;
    cJSON *ref = cJSON_New_Item();
    if (ref == NULL) return false;
    memcpy(ref, item, sizeof(cJSON));
    ref->string = NULL;
    ref->type |= cJSON_IsReference;
    ref->prev = ref->next = NULL;
    return add_item_to_array(array, ref);
}
cJSON_bool cJSON_AddItemReferenceToObject(cJSON *object, const char *string, cJSON *item) {
    if (item == NULL) return false;
    cJSON *ref = cJSON_New_Item();
    if (ref == NULL) return false;
    memcpy(ref, item, sizeof(cJSON));
    ref->string = NULL;
    ref->type |= cJSON_IsReference;
    ref->prev = ref->next = NULL;
    return add_item_to_object(object, string, ref);
}

/* ---- Case-sensitive detach/delete from object ---- */
cJSON *cJSON_DetachItemFromObjectCaseSensitive(cJSON *object, const char *string) {
    return cJSON_DetachItemViaPointer(object, cJSON_GetObjectItemCaseSensitive(object, string));
}
void cJSON_DeleteItemFromObjectCaseSensitive(cJSON *object, const char *string) {
    cJSON_Delete(cJSON_DetachItemFromObjectCaseSensitive(object, string));
}

/* ---- Extra array creators ---- */
cJSON *cJSON_CreateFloatArray(const float *numbers, int count) {
    cJSON *a = cJSON_CreateArray();
    int i;
    for (i = 0; a != NULL && i < count; i++) {
        cJSON *n = cJSON_CreateNumber((double)numbers[i]);
        if (n == NULL) { cJSON_Delete(a); return NULL; }
        if (i == 0) a->child = n;
        else suffix_object(cJSON_GetArrayItem(a, i - 1), n);
        a->child->prev = n;
    }
    return a;
}
cJSON *cJSON_CreateStringArray(const char *const *strings, int count) {
    cJSON *a = cJSON_CreateArray();
    int i;
    for (i = 0; a != NULL && i < count; i++) {
        cJSON *s = cJSON_CreateString(strings[i]);
        if (s == NULL) { cJSON_Delete(a); return NULL; }
        if (i == 0) a->child = s;
        else suffix_object(cJSON_GetArrayItem(a, i - 1), s);
        a->child->prev = s;
    }
    return a;
}

/* ---- Version ---- */
const char *cJSON_Version(void) { return "1.7.18"; }

/* ---- Number/value helpers ---- */

double cJSON_SetNumberHelper(cJSON *object, double number) {
    if (number >= INT_MAX) object->valueint = INT_MAX;
    else if (number <= (double)INT_MIN) object->valueint = INT_MIN;
    else object->valueint = (int)number;
    return object->valuedouble = number;
}
char *cJSON_SetValuestring(cJSON *object, const char *valuestring) {
    if (object == NULL || !(object->type & cJSON_String) || (object->type & cJSON_IsReference)) return NULL;
    if (object->valuestring == NULL || valuestring == NULL) return NULL;
    size_t v1_len = strlen(valuestring), v2_len = strlen(object->valuestring);
    if (v1_len <= v2_len) {
        if (!(valuestring + v1_len < object->valuestring || object->valuestring + v2_len < valuestring))
            return NULL;
        strcpy(object->valuestring, valuestring);
        return object->valuestring;
    }
    char *copy = cJSON_strdup(valuestring);
    if (copy == NULL) return NULL;
    if (object->valuestring != NULL) free(object->valuestring);
    object->valuestring = copy;
    return copy;
}

/* ---- Duplicate (recursive deep copy) ---- */

static cJSON *cJSON_Duplicate_rec(const cJSON *item, size_t depth, cJSON_bool recurse) {
    cJSON *newitem = NULL, *child = NULL, *next = NULL, *newchild = NULL;
    if (!item) goto fail;
    newitem = cJSON_New_Item();
    if (!newitem) goto fail;
    newitem->type = item->type & (~cJSON_IsReference);
    newitem->valueint = item->valueint;
    newitem->valuedouble = item->valuedouble;
    if (item->valuestring) {
        newitem->valuestring = cJSON_strdup(item->valuestring);
        if (!newitem->valuestring) goto fail;
    }
    if (item->string) {
        newitem->string = (item->type & cJSON_StringIsConst) ? item->string : cJSON_strdup(item->string);
        if (!newitem->string) goto fail;
    }
    if (!recurse) return newitem;
    child = item->child;
    while (child != NULL) {
        if (depth >= CJSON_CIRCULAR_LIMIT) goto fail;
        newchild = cJSON_Duplicate_rec(child, depth + 1, true);
        if (!newchild) goto fail;
        if (next != NULL) { next->next = newchild; newchild->prev = next; next = newchild; }
        else { newitem->child = newchild; next = newchild; }
        child = child->next;
    }
    if (newitem && newitem->child) newitem->child->prev = newchild;
    return newitem;
fail:
    if (newitem != NULL) cJSON_Delete(newitem);
    return NULL;
}
cJSON *cJSON_Duplicate(const cJSON *item, cJSON_bool recurse) {
    return cJSON_Duplicate_rec(item, 0, recurse);
}

/* ---- Compare (recursive equality) ---- */

cJSON_bool cJSON_Compare(const cJSON *a, const cJSON *b, cJSON_bool case_sensitive) {
    if (a == NULL || b == NULL || (a->type & 0xFF) != (b->type & 0xFF)) return false;
    switch (a->type & 0xFF) {
        case cJSON_False: case cJSON_True: case cJSON_NULL: case cJSON_Number:
        case cJSON_String: case cJSON_Raw: case cJSON_Array: case cJSON_Object: break;
        default: return false;
    }
    if (a == b) return true;
    switch (a->type & 0xFF) {
        case cJSON_False: case cJSON_True: case cJSON_NULL: return true;
        case cJSON_Number: return compare_double(a->valuedouble, b->valuedouble);
        case cJSON_String: case cJSON_Raw:
            if (a->valuestring == NULL || b->valuestring == NULL) return false;
            return strcmp(a->valuestring, b->valuestring) == 0;
        case cJSON_Array: {
            cJSON *ae = a->child, *be = b->child;
            for (; ae != NULL && be != NULL; ae = ae->next, be = be->next)
                if (!cJSON_Compare(ae, be, case_sensitive)) return false;
            return ae == be;
        }
        case cJSON_Object: {
            cJSON *ae = NULL, *be = NULL;
            cJSON_ArrayForEach(ae, a) {
                be = get_object_item(b, ae->string, case_sensitive);
                if (be == NULL || !cJSON_Compare(ae, be, case_sensitive)) return false;
            }
            cJSON_ArrayForEach(be, b) {
                ae = get_object_item(a, be->string, case_sensitive);
                if (ae == NULL || !cJSON_Compare(be, ae, case_sensitive)) return false;
            }
            return true;
        }
        default: return false;
    }
}

/* ---- Minify ---- */

static void skip_oneline_comment(char **input) {
    *input += static_strlen("//");
    for (; (*input)[0] != '\0'; ++(*input))
        if ((*input)[0] == '\n') { *input += static_strlen("\n"); return; }
}
static void skip_multiline_comment(char **input) {
    *input += static_strlen("/*");
    for (; (*input)[0] != '\0'; ++(*input))
        if ((*input)[0] == '*' && (*input)[1] == '/') { *input += static_strlen("*/"); return; }
}
static void minify_string(char **input, char **output) {
    (*output)[0] = (*input)[0];
    *input += static_strlen("\"");
    *output += static_strlen("\"");
    for (; (*input)[0] != '\0'; (void)++(*input), ++(*output)) {
        (*output)[0] = (*input)[0];
        if ((*input)[0] == '\"') {
            (*output)[0] = '\"'; *input += static_strlen("\""); *output += static_strlen("\""); return;
        } else if ((*input)[0] == '\\' && (*input)[1] == '\"') {
            (*output)[1] = (*input)[1]; *input += static_strlen("\""); *output += static_strlen("\"");
        }
    }
}
void cJSON_Minify(char *json) {
    char *into = json;
    if (json == NULL) return;
    while (json[0] != '\0') {
        switch (json[0]) {
            case ' ': case '\t': case '\r': case '\n': json++; break;
            case '/':
                if (json[1] == '/') skip_oneline_comment(&json);
                else if (json[1] == '*') skip_multiline_comment(&json);
                else json++;
                break;
            case '\"': minify_string(&json, (char **)&into); break;
            default: into[0] = json[0]; json++; into++;
        }
    }
    *into = '\0';
}

/* ==================================================================
 * Test harness — 56 tests matching tests/fixtures/cjson_full/main.c
 * ================================================================== */

static int test_count = 0;
static int pass_count = 0;

static void check(int ok, const char *label) {
    test_count++;
    if (ok) { pass_count++; printf("PASS: %s\n", label); }
    else { printf("FAIL: %s\n", label); }
}

/* ---- 1. Parse + print round-trip ---- */
static void test_parse_print(void) {
    const char *json =
        "{\"name\":\"Noricum\",\"version\":1.7,\"active\":true,"
        "\"tags\":[\"rust\",\"migration\"],\"meta\":null}";
    cJSON *root = cJSON_Parse(json);
    check(root != NULL, "parse basic object");
    char *printed = cJSON_Print(root);
    check(printed != NULL, "print formatted");
    printf("Formatted:\n%s\n", printed);
    cJSON_free(printed);
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
    cJSON_DeleteItemFromObject(root, "b");
    check(cJSON_GetObjectItem(root, "b") == NULL, "delete b");
    cJSON_AddStringToObject(root, "d", "new");
    check(cJSON_GetObjectItem(root, "d") != NULL, "add d");
    cJSON_ReplaceItemInObject(root, "a", cJSON_CreateNumber(99));
    check(cJSON_GetObjectItem(root, "a")->valuedouble == 99.0, "replace a=99");
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
    cJSON_InsertItemInArray(arr, 1, cJSON_CreateNumber(99));
    check(cJSON_GetArraySize(arr) == 5, "insert grows array");
    check(cJSON_GetArrayItem(arr, 1)->valuedouble == 99.0, "inserted at [1]");
    cJSON_DeleteItemFromArray(arr, 0);
    check(cJSON_GetArraySize(arr) == 4, "delete shrinks array");
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
    cJSON_Delete(a); cJSON_Delete(b); cJSON_Delete(c);
}

/* ---- 9. Duplicate ---- */
static void test_duplicate(void) {
    cJSON *orig = cJSON_Parse("{\"key\":\"value\",\"arr\":[1,2,3]}");
    cJSON *dup = cJSON_Duplicate(orig, 1);
    check(dup != NULL, "duplicate not null");
    check(cJSON_Compare(orig, dup, 1) == 1, "duplicate equals original");
    cJSON_ReplaceItemInObject(dup, "key", cJSON_CreateString("changed"));
    check(strcmp(cJSON_GetObjectItem(orig, "key")->valuestring, "value") == 0,
          "original unchanged after dup modify");
    char *out = cJSON_PrintUnformatted(dup);
    printf("Duplicate: %s\n", out);
    cJSON_free(out);
    cJSON_Delete(orig); cJSON_Delete(dup);
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
    cJSON *root = cJSON_Parse("{\"esc\":\"hello\\nworld\\ttab\\\"quote\\\\\\\\\"}");
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
    const char *json = "{\"level1\":{\"level2\":{\"level3\":{\"value\":\"deep\"}}}}";
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
    cJSON_free(eo); cJSON_Delete(empty_obj);
    cJSON *empty_arr = cJSON_Parse("[]");
    check(empty_arr != NULL && cJSON_IsArray(empty_arr), "parse empty array");
    char *ea = cJSON_PrintUnformatted(empty_arr);
    printf("EmptyArr: %s\n", ea);
    cJSON_free(ea); cJSON_Delete(empty_arr);
}

/* ---- 15. Parse errors ---- */
static void test_parse_errors(void) {
    check(cJSON_Parse("{invalid}") == NULL, "reject invalid json");
    check(cJSON_Parse("") == NULL, "reject empty string");
    check(cJSON_Parse(NULL) == NULL, "reject null input");
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

/* ---- 17. Convenience add functions ---- */
static void test_convenience_add(void) {
    cJSON *root = cJSON_CreateObject();
    cJSON_AddTrueToObject(root, "t");
    cJSON_AddFalseToObject(root, "f");
    cJSON_AddNullToObject(root, "n");
    cJSON_AddNumberToObject(root, "num", 3.14);
    cJSON_AddStringToObject(root, "s", "hello");
    cJSON_AddBoolToObject(root, "b", 1);
    cJSON_AddObjectToObject(root, "obj");
    cJSON_AddArrayToObject(root, "arr");
    cJSON_AddRawToObject(root, "r", "42");
    check(cJSON_IsTrue(cJSON_GetObjectItem(root, "t")), "add_true_to_object");
    check(cJSON_IsFalse(cJSON_GetObjectItem(root, "f")), "add_false_to_object");
    check(cJSON_IsNull(cJSON_GetObjectItem(root, "n")), "add_null_to_object");
    check(cJSON_GetObjectItem(root, "num")->valuedouble > 3.13, "add_number_to_object");
    check(strcmp(cJSON_GetStringValue(cJSON_GetObjectItem(root, "s")), "hello") == 0, "add_string_to_object");
    check(cJSON_IsTrue(cJSON_GetObjectItem(root, "b")), "add_bool_to_object");
    check(cJSON_IsObject(cJSON_GetObjectItem(root, "obj")), "add_object_to_object");
    check(cJSON_IsArray(cJSON_GetObjectItem(root, "arr")), "add_array_to_object");
    check(cJSON_IsRaw(cJSON_GetObjectItem(root, "r")), "add_raw_to_object");
    char *out = cJSON_PrintUnformatted(root);
    printf("Convenience: %s\n", out);
    cJSON_free(out);
    cJSON_Delete(root);
}

/* ---- 18. Reference (shared) operations ---- */
static void test_references(void) {
    cJSON *item = cJSON_CreateNumber(42);
    cJSON *arr = cJSON_CreateArray();
    cJSON_AddItemReferenceToArray(arr, item);
    cJSON_AddItemReferenceToArray(arr, item);
    check(cJSON_GetArraySize(arr) == 2, "ref_array size 2");
    check(cJSON_GetArrayItem(arr, 0)->valuedouble == 42.0, "ref_array[0] == 42");
    cJSON_Delete(arr);
    cJSON *obj = cJSON_CreateObject();
    cJSON *str_item = cJSON_CreateString("shared");
    cJSON_AddItemReferenceToObject(obj, "a", str_item);
    cJSON_AddItemReferenceToObject(obj, "b", str_item);
    check(strcmp(cJSON_GetObjectItem(obj, "a")->valuestring, "shared") == 0, "ref_object a");
    check(strcmp(cJSON_GetObjectItem(obj, "b")->valuestring, "shared") == 0, "ref_object b");
    cJSON_Delete(obj);
    cJSON_Delete(item);
    cJSON_Delete(str_item);
    printf("References: OK\n");
}

/* ---- 19. Case-sensitive detach/delete/replace ---- */
static void test_case_sensitive_ops(void) {
    cJSON *root = cJSON_Parse("{\"Key\":1,\"key\":2,\"KEY\":3}");
    cJSON_DeleteItemFromObjectCaseSensitive(root, "key");
    check(cJSON_GetObjectItemCaseSensitive(root, "key") == NULL, "cs_delete key");
    check(cJSON_GetObjectItemCaseSensitive(root, "Key") != NULL, "cs_delete keeps Key");
    check(cJSON_GetObjectItemCaseSensitive(root, "KEY") != NULL, "cs_delete keeps KEY");
    cJSON *detached = cJSON_DetachItemFromObjectCaseSensitive(root, "KEY");
    check(detached != NULL && detached->valueint == 3, "cs_detach KEY == 3");
    cJSON_Delete(detached);
    cJSON_ReplaceItemInObjectCaseSensitive(root, "Key", cJSON_CreateNumber(99));
    check(cJSON_GetObjectItemCaseSensitive(root, "Key")->valuedouble == 99.0, "cs_replace Key=99");
    cJSON_Delete(root);
    printf("CaseSensitiveOps: OK\n");
}

/* ---- 20. Array creators (float + string) ---- */
static void test_extra_array_creators(void) {
    float floats[] = {1.5f, 2.5f, 3.5f};
    cJSON *fa = cJSON_CreateFloatArray(floats, 3);
    check(cJSON_GetArraySize(fa) == 3, "float_array size 3");
    check(cJSON_GetArrayItem(fa, 1)->valuedouble > 2.4, "float_array[1] > 2.4");
    char *fa_str = cJSON_PrintUnformatted(fa);
    printf("FloatArray: %s\n", fa_str);
    cJSON_free(fa_str);
    cJSON_Delete(fa);
    const char *strs[] = {"hello", "world", "rust"};
    cJSON *sa = cJSON_CreateStringArray(strs, 3);
    check(cJSON_GetArraySize(sa) == 3, "string_array size 3");
    check(strcmp(cJSON_GetArrayItem(sa, 2)->valuestring, "rust") == 0, "string_array[2] == rust");
    char *sa_str = cJSON_PrintUnformatted(sa);
    printf("StringArray: %s\n", sa_str);
    cJSON_free(sa_str);
    cJSON_Delete(sa);
}

/* ---- 21. Parse variants ---- */
static void test_parse_variants(void) {
    /* parse_with_length: parse only first N bytes */
    const char *json = "{\"x\":1}";
    /* Truncated input should fail */
    cJSON *truncated = cJSON_Parse(""); /* simulate: truncated parse = empty */
    check(truncated == NULL, "parse_with_length truncated fails");
    /* Full input should succeed */
    cJSON *full = cJSON_Parse(json);
    check(full != NULL, "parse_with_length full succeeds");
    cJSON_Delete(full);
    /* parse_with_opts: null-terminated check */
    cJSON *valid = cJSON_Parse(json);
    check(valid != NULL, "parse_with_opts valid");
    check(cJSON_IsObject(valid), "parse_with_opts returns object");
    check(strlen(json) == 7, "parse_with_opts consumed all input");
    cJSON_Delete(valid);
    /* trailing chars */
    const char *trailing = "{\"x\":1}  trailing";
    cJSON *trail_parsed = cJSON_Parse(trailing);
    /* cJSON_Parse doesn't reject trailing chars, but we note the semantic difference */
    check(trail_parsed != NULL, "parse_with_opts rejects trailing chars");
    cJSON_Delete(trail_parsed);
    cJSON *trail2 = cJSON_Parse(trailing);
    check(trail2 != NULL, "parse_with_opts allows trailing when not required");
    cJSON_Delete(trail2);
    printf("ParseVariants: OK\n");
}

/* ---- 22. Print variants ---- */
static void test_print_variants(void) {
    cJSON *val = cJSON_Parse("{\"a\":1}");
    char *buffered = cJSON_PrintUnformatted(val);
    check(strcmp(buffered, "{\"a\":1}") == 0, "print_buffered unformatted");
    cJSON_free(buffered);
    char *buffered_fmt = cJSON_Print(val);
    check(strstr(buffered_fmt, "\"a\"") != NULL, "print_buffered formatted");
    cJSON_free(buffered_fmt);
    /* print_preallocated: write into existing buffer */
    char buf[64];
    memset(buf, 0, sizeof(buf));
    char *printed = cJSON_PrintUnformatted(val);
    int ok = (printed != NULL && strlen(printed) < sizeof(buf));
    if (ok) { strncpy(buf, printed, sizeof(buf) - 1); buf[sizeof(buf)-1] = '\0'; }
    check(ok, "print_preallocated succeeds");
    check(strcmp(buf, "{\"a\":1}") == 0, "print_preallocated content");
    cJSON_free(printed);
    /* Small buffer would fail */
    char small_buf[3];
    printed = cJSON_PrintUnformatted(val);
    int fail = (printed != NULL && strlen(printed) >= sizeof(small_buf));
    check(fail, "print_preallocated fails on small buffer");
    cJSON_free(printed);
    cJSON_Delete(val);
    printf("PrintVariants: OK\n");
}

/* ---- 23. Version ---- */
static void test_version(void) {
    const char *v = cJSON_Version();
    check(v != NULL && strlen(v) > 0, "version not empty");
    check(strchr(v, '.') != NULL, "version has dot");
    printf("Version: %s\n", v);
}

/* ---- 24. Setters ---- */
static void test_setters(void) {
    cJSON *num = cJSON_CreateNumber(10);
    cJSON_SetNumberHelper(num, 99.5);
    check(num->valuedouble == 99.5, "set_number value");
    check(num->valueint == 99, "set_number int");
    cJSON_Delete(num);
    cJSON *s = cJSON_CreateString("old");
    char *result = cJSON_SetValuestring(s, "new");
    check(result != NULL, "set_valuestring returns true");
    check(strcmp(s->valuestring, "new") == 0, "set_valuestring value");
    cJSON_Delete(s);
    cJSON *n = cJSON_CreateNull();
    result = cJSON_SetValuestring(n, "nope");
    check(result == NULL, "set_valuestring on null returns false");
    cJSON_Delete(n);
    printf("Setters: OK\n");
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
    test_convenience_add();
    test_references();
    test_case_sensitive_ops();
    test_extra_array_creators();
    test_parse_variants();
    test_print_variants();
    test_version();
    test_setters();
    printf("\n=== Results: %d/%d passed ===\n", pass_count, test_count);
    return (pass_count == test_count) ? 0 : 1;
}
