#include "base64.h"

namespace base64 {

static int b64val(char c) {
    if (c >= 'A' && c <= 'Z') return (c - 'A');          // 0..25
    if (c >= 'a' && c <= 'z') return (c - 'a' + 26);     // 26..51
    if (c >= '0' && c <= '9') return (c - '0' + 52);     // 52..61
    if (c == '+') return 62;
    if (c == '/') return 63;
    return -1;
}

bool decode(const std::string &in, std::vector<uint8_t> &out) {
    // Strip whitespace into a compact buffer.
    std::string s;
    s.reserve(in.size());
    for (char c : in) {
        if (c != ' ' && c != '\t' && c != '\r' && c != '\n')
            s += c;
    }

    if (s.empty() || (s.size() % 4) != 0) return false;

    // Validate alphabet and padding placement (= only at the very end, max 2).
    int pad = 0;
    for (size_t i = 0; i < s.size(); i++) {
        char c = s[i];
        if (c == '=') {
            pad++;
            if (pad > 2) return false;
        } else {
            if (pad > 0) return false;      // data char after padding
            if (b64val(c) < 0) return false;
        }
    }

    out.clear();
    out.reserve((s.size() * 3) / 4);

    for (size_t i = 0; i < s.size(); i += 4) {
        int v0 = (s[i + 0] == '=') ? 0 : b64val(s[i + 0]);
        int v1 = (s[i + 1] == '=') ? 0 : b64val(s[i + 1]);
        int v2 = (s[i + 2] == '=') ? 0 : b64val(s[i + 2]);
        int v3 = (s[i + 3] == '=') ? 0 : b64val(s[i + 3]);

        out.push_back((uint8_t)((v0 << 2) | (v1 >> 4)));
        if (s[i + 2] != '=') {
            out.push_back((uint8_t)((v1 << 4) | (v2 >> 2)));
            if (s[i + 3] != '=') {
                out.push_back((uint8_t)((v2 << 6) | v3));
            }
        }
    }

    return true;
}

} // namespace base64
