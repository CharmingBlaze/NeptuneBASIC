/*
 * A NeptuneBASIC plugin in C.
 *
 * Build (pick your platform), then copy the library into a "plugins" folder:
 *   Windows (MSVC): cl /LD /I.. hello_plugin.c /Fe:hello_c.dll
 *   Windows (MinGW): gcc -shared -I.. hello_plugin.c -o hello_c.dll
 *   Linux:          gcc -shared -fPIC -I.. hello_plugin.c -o libhello_c.so
 *   macOS:          clang -shared -I.. hello_plugin.c -o libhello_c.dylib
 *
 * Then in BASIC:
 *   Print CTimesTable(7, 6)      ; 42
 *   Print CShout("hello")        ; HELLO!
 */
#include <ctype.h>
#include <string.h>
#include "neptune_plugin.h"

static int32_t times_table(void *user, const NbValue *args, int32_t argc, NbValue *result) {
    (void)user;
    (void)argc;
    result->kind = NB_INT;
    result->i = args[0].i * args[1].i;
    return 0;
}

static char shout_buffer[1024];

static int32_t shout(void *user, const NbValue *args, int32_t argc, NbValue *result) {
    (void)user;
    (void)argc;
    size_t n = args[0].len;
    if (n + 2 > sizeof shout_buffer) {
        result->kind = NB_STRING;
        result->s = "CShout: text too long";
        result->len = 0;
        return 1; /* a runtime error the program can catch */
    }
    for (size_t i = 0; i < n; i++) shout_buffer[i] = (char)toupper((unsigned char)args[0].s[i]);
    shout_buffer[n] = '!';
    result->kind = NB_STRING;
    result->s = shout_buffer;
    result->len = n + 1;
    return 0;
}

NEPTUNE_EXPORT int32_t neptune_plugin_init(const NbHost *host) {
    if (host->api_version != NEPTUNE_PLUGIN_API_VERSION) return 1;
    host->add_command(host->host, "%CTimesTable%a%b", times_table, NULL);
    host->add_command(host->host, "$CShout$text", shout, NULL);
    host->add_constant_int(host->host, "C_PLUGIN_VERSION", 1);
    return 0;
}
