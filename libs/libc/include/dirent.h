#ifndef _DIRENT_H
#define _DIRENT_H

#include <sys/types.h>

#ifdef __cplusplus
extern "C" {
#endif

struct dirent {
    unsigned long d_ino;
    off_t d_off;
    unsigned short d_reclen;
    unsigned char d_type;
    char d_name[256];
};

typedef struct __DIR DIR;

DIR *opendir(const char *name);
struct dirent *readdir(DIR *dirp);
int closedir(DIR *dirp);

#ifdef __cplusplus
}
#endif

#endif
