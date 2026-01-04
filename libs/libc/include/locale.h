#ifndef _LOCALE_H
#define _LOCALE_H

#define LC_ALL      6
#define LC_COLLATE  3
#define LC_CTYPE    0
#define LC_MONETARY 4
#define LC_NUMERIC  1
#define LC_TIME     2

extern char *setlocale(int category, const char *locale);

#endif
