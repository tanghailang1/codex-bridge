#include "codex_bridge.h"

#include <stdint.h>
#include <stdio.h>

int main(void) {
    uint32_t version = codex_bridge_login_abi_version();
    if (version != 1) {
        fprintf(stderr, "unexpected ABI version: %u\n", version);
        return 1;
    }
    printf("codex bridge login ABI version: %u\n", version);
    printf("Codex commit: %s\n", codex_bridge_codex_commit());
    return 0;
}
