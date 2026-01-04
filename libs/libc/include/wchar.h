#ifndef _WCHAR_H
#define _WCHAR_H

#include <stddef.h>

typedef int wchar_t;
typedef int wint_t;

#define WEOF (-1)

extern int wcwidth(wchar_t wc);
extern int wctomb(char *s, wchar_t wc);

#endif
