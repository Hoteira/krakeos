@echo off
set CC=clang
set CFLAGS=-target x86_64-unknown-none-elf -ffreestanding -fno-stack-protector -fPIC -I../../../libs/libc/include -I. -DNANO_TINY -DHAVE_CONFIG_H -O2 -mno-red-zone -mno-mmx -mno-sse -mno-sse2
set LDFLAGS=-pie --entry _start

set SOURCES=browser.c chars.c color.c cut.c files.c global.c help.c history.c move.c nano.c prompt.c rcfile.c search.c text.c utils.c winio.c

if not exist build mkdir build

echo Compiling nano...
for %%f in (%SOURCES%) do (
    echo   %%f
    %CC% %CFLAGS% -c %%f -o build/%%~nf.o
)

echo Linking nano.elf...
ld.lld %LDFLAGS% -o nano.elf build/*.o ../../../target/bits64pie/release/liblibc.a ../../../target/bits64pie/release/libstd.a

echo Build complete.
