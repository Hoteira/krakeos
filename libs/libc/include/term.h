#ifndef _TERM_H
#define _TERM_H

#ifdef __cplusplus
extern "C" {
#endif

char *tgetstr(const char *id, char **area);

#ifdef __cplusplus
}
#endif

#endif
