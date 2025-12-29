#ifndef _SYS_STAT_H
#define _SYS_STAT_H
#include <sys/types.h>
struct stat { off_t st_size; mode_t st_mode; };
extern int stat(const char *path, struct stat *buf);
extern int mkdir(const char *path, mode_t mode);
#endif
