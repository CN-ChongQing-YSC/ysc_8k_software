#ifndef YSC_BASE64_H
#define YSC_BASE64_H

#include <cstdint>
#include <string>
#include <vector>

// Minimal standard Base64 decoder. Used to ship firmware bytes through the
// existing text-only JSON pipe protocol between the Electron main process
// and the C++ driver — the firmware buffer is encoded as Base64 on the
// Electron side and decoded here before being handed to the IAP upgraders.
namespace base64 {
    // Decode a standard (padded) Base64 string into `out`. Whitespace
    // (space/tab/CR/LF) is silently skipped. Returns false on malformed
    // input: characters outside the Base64 alphabet, bad padding (= only
    // allowed as the last 1-2 chars of a 4-byte group), or a length that
    // is not a multiple of 4 after whitespace stripping.
    bool decode(const std::string &in, std::vector<uint8_t> &out);
}

#endif // YSC_BASE64_H
