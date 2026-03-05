// Singly linked list - hard difficulty (heap allocation, pointers)
#include <stdio.h>
#include <stdlib.h>

typedef struct Node {
    int value;
    struct Node *next;
} Node;

Node *node_new(int value) {
    Node *n = (Node *)malloc(sizeof(Node));
    if (!n) return NULL;
    n->value = value;
    n->next = NULL;
    return n;
}

void list_push(Node **head, int value) {
    Node *n = node_new(value);
    if (!n) return;
    n->next = *head;
    *head = n;
}

int list_sum(const Node *head) {
    int sum = 0;
    while (head) {
        sum += head->value;
        head = head->next;
    }
    return sum;
}

void list_free(Node *head) {
    while (head) {
        Node *tmp = head;
        head = head->next;
        free(tmp);
    }
}

int main(void) {
    Node *list = NULL;
    list_push(&list, 10);
    list_push(&list, 20);
    list_push(&list, 30);
    printf("%d\n", list_sum(list));
    list_free(list);
    return 0;
}
