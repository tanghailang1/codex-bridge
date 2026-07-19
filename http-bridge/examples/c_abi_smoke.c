#include "codex_http_bridge.h"

#include <stdint.h>
#include <stdio.h>

int main(void) {
    uint32_t version = codex_http_bridge_abi_version();
    if (version != 1) {
        fprintf(stderr, "unexpected HTTP ABI version: %u\n", version);
        return 1;
    }
    printf("codex HTTP bridge ABI version: %u\n", version);
    printf("Codex commit: %s\n", codex_http_bridge_codex_commit());
    return 0;
}
