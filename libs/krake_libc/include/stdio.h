#ifndef _STDIO_H
#define _STDIO_H

#include <stddef.h>
#include <stdarg.h>

typedef void FILE;

#define stderr ((FILE*)2)
#define stdout ((FILE*)1)
#define stdin  ((FILE*)0)

extern int printf(const char *format, ...);
extern int fprintf(FILE *stream, const char *format, ...);
extern int vfprintf(FILE *stream, const char *format, va_list ap);
extern int snprintf(char *str, size_t size, const char *format, ...);
extern int vsnprintf(char *str, size_t size, const char *format, va_list ap);
extern int sscanf(const char *str, const char *format, ...);
extern int putchar(int c);
extern int puts(const char *s);
extern int fflush(FILE *stream);
extern int remove(const char *pathname);
extern int rename(const char *oldpath, const char *newpath);

extern FILE *fopen(const char *filename, const char *mode);
extern int fclose(FILE *stream);
extern size_t fread(void *ptr, size_t size, size_t nmemb, FILE *stream);
extern size_t fwrite(const void *ptr, size_t size, size_t nmemb, FILE *stream);
extern int fseek(FILE *stream, long offset, int whence);
extern long ftell(FILE *stream);

#define SEEK_SET 0
#define SEEK_CUR 1
#define SEEK_END 2

#endif
