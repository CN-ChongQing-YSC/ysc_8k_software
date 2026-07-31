#ifndef KMBOXNET_PROTO_H
#define KMBOXNET_PROTO_H

#include <stdint.h>

#pragma pack(push, 1)

// Packet header (16 bytes)
struct kmboxnet_header_t {
    uint32_t mac;       // Device MAC/UUID
    uint32_t rand;      // Random / parameter
    uint32_t indexpts;  // Sequence number
    uint32_t cmd;       // Command code
};

// Mouse payload (56 bytes)
struct soft_mouse_t {
    uint32_t button;    // Button bitmask
    int32_t  x;         // X movement
    int32_t  y;         // Y movement
    int32_t  wheel;     // Wheel delta
    int32_t  point[10]; // Bezier control points / auto-move segments
};

// Keyboard payload (12 bytes)
struct soft_keyboard_t {
    uint8_t ctrl;       // Modifier keys bitmask
    uint8_t resvel;     // Reserved
    uint8_t button[10]; // Key codes (HID scan codes)
};

// Monitor report: mouse (9 bytes)
struct mouse_report_t {
    uint8_t  report_id; // 0x01
    uint8_t  buttons;   // Button bitmask
    int16_t  x;         // X delta
    int16_t  y;         // Y delta
    int16_t  wheel;     // Wheel delta
};

// Monitor report: keyboard (12 bytes)
struct keyboard_report_t {
    uint8_t  report_id; // 0x02
    uint8_t  buttons;   // Modifier keys
    uint8_t  data[10];  // Key codes
};

// Monitor packet (matches kmboxnet wire format: mouse then keyboard, no prefix)
struct monitor_packet_t {
    mouse_report_t   mouse;
    keyboard_report_t kb;
};

#pragma pack(pop)

// Command codes
#define CMD_CONNECT        0xaf3c2828
#define CMD_MOUSE_MOVE     0xaede7345
#define CMD_MOUSE_LEFT     0x9823AE8D
#define CMD_MOUSE_MIDDLE   0x97a3AE8D
#define CMD_MOUSE_RIGHT    0x238d8212
#define CMD_MOUSE_WHEEL    0xffeead38
#define CMD_KEYBOARD_ALL   0x123c2c2f
#define CMD_MONITOR        0x27388020
#define CMD_MOUSE_AUTOMOVE 0xaede7346
#define CMD_BAZER_MOVE     0xa238455a
#define CMD_REBOOT         0xaa8855aa

// Default device MAC
#define DEFAULT_MAC        0x00000001

// Monitor port magic
#define MONITOR_PORT_MAGIC 0xaa550000

#endif // KMBOXNET_PROTO_H
