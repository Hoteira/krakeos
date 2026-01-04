#ifndef _WCTYPE_H
#define _WCTYPE_H

#include <wchar.h>

extern int iswspace(wint_t wc);
extern int iswalnum(wint_t wc);
extern int iswblank(wint_t wc);
extern int iswpunct(wint_t wc);
extern int iswprint(wint_t wc);
extern wint_t towlower(wint_t wc);
extern wint_t towupper(wint_t wc);

#endif
