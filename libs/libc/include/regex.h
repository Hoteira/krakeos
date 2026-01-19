#ifndef _REGEX_H
#define _REGEX_H

#include <sys/types.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    int re_nsub;
    void *re_guts;
} regex_t;

typedef int regoff_t;

typedef struct {
    regoff_t rm_so;
    regoff_t rm_eo;
} regmatch_t;

/* POSIX regex flags */
#define REG_EXTENDED 1
#define REG_ICASE    2
#define REG_NOSUB    4
#define REG_NEWLINE  8

/* POSIX regex error codes */
#define REG_NOMATCH 1
#define REG_BADPAT  2

/* POSIX regex match flags */
#define REG_NOTBOL 1
#define REG_NOTEOL 2
#define REG_STARTEND 4

int regcomp(regex_t *preg, const char *regex, int cflags);
int regexec(const regex_t *preg, const char *string, size_t nmatch, regmatch_t pmatch[], int eflags);
size_t regerror(int errcode, const regex_t *preg, char *errbuf, size_t errbuf_size);
void regfree(regex_t *preg);

#ifdef __cplusplus
}
#endif

#endif
