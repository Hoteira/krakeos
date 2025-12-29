#ifndef _STDLIB_H
#define _STDLIB_H

#include <stddef.h>

extern void *malloc(size_t size);
extern void free(void *ptr);
extern void *calloc(size_t nmemb, size_t size);
extern void *realloc(void *ptr, size_t size);
extern void exit(int status);
extern char *getenv(const char *name);
extern int atoi(const char *nptr);
extern double atof(const char *nptr);
extern int abs(int j);
extern int system(const char *command);

#endif
