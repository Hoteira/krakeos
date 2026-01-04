#ifndef _SYS_STAT_H
#define _SYS_STAT_H
#include <sys/types.h>
#include <stdint.h>

struct stat {
    ino_t     st_ino;     /* 0  - 8 bytes */
    mode_t    st_mode;    /* 8  - 4 bytes */
    uint32_t  __pad0;     /* 12 - 4 bytes */
    off_t     st_size;    /* 16 - 8 bytes */
    uid_t     st_uid;     /* 24 - 4 bytes */
    gid_t     st_gid;     /* 28 - 4 bytes */
};

#define S_IFMT  0170000
#define S_IFDIR 0040000
#define S_IFCHR 0020000
#define S_IFBLK 0060000
#define S_IFREG 0100000
#define S_IFLNK 0120000
#define S_IFIFO 0010000

#define S_ISDIR(m) (((m) & S_IFMT) == S_IFDIR)
#define S_ISCHR(m) (((m) & S_IFMT) == S_IFCHR)
#define S_ISBLK(m) (((m) & S_IFMT) == S_IFBLK)
#define S_ISREG(m) (((m) & S_IFMT) == S_IFREG)
#define S_ISLNK(m) (((m) & S_IFMT) == S_IFLNK)
#define S_ISFIFO(m) (((m) & S_IFMT) == S_IFIFO)

#define S_IRUSR 0400
#define S_IWUSR 0200
#define S_IXUSR 0100
#define S_IRGRP 0040
#define S_IWGRP 0020
#define S_IXGRP 0010
#define S_IROTH 0004
#define S_IWOTH 0002
#define S_IXOTH 0001

extern int stat(const char *path, struct stat *buf);
extern int mkdir(const char *path, mode_t mode);
#endif