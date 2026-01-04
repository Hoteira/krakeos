#ifndef _STDLIB_H
#define _STDLIB_H

#include <stddef.h>

#define MB_CUR_MAX 4

extern void *malloc(size_t size);
extern void free(void *ptr);
extern void *calloc(size_t nmemb, size_t size);
extern void *realloc(void *ptr, size_t size);
extern void exit(int status);
extern char *getenv(const char *name);
extern char *realpath(const char *path, char *resolved_path);
extern int mkstemp(char *template);
extern int atoi(const char *nptr);
extern long strtol(const char *nptr, char **endptr, int base);
extern double atof(const char *nptr);
extern int abs(int j);
extern int system(const char *command);
extern int putenv(char *string);
extern int mkstemps(char *template, int suffixlen);

#endif
