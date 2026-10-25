; p63_hw.inc

; Memory map
SP_RESET        EQU $FBB5FF     ; stack pointer after reset
MAIN_BASE       EQU $000100
MAIN_TOP        EQU $FBB5FF
VRAM_BASE       EQU $FDFE00
VRAM_END        EQU $FFFDFF
OAM_BASE        EQU $FDF600
OAM_END         EQU $FDFDFF
ARAM_BASE       EQU $FCF600
ARAM_END        EQU $FDF5FF
AUDIO_IO_BASE   EQU $FFFF00
AUDIO_IO_END    EQU $FFFF7F
VIDEO_IO_BASE   EQU $FFFF80
VIDEO_IO_END    EQU $FFFFBF
KB_IO_BASE      EQU $FFFFC0
KB_IO_END       EQU $FFFFDF

; Vectors
VEC_SWI         EQU $FFFFF5
VEC_NMI         EQU $FFFFF8
VEC_IRQ         EQU $FFFFFB
VEC_FIRQ        EQU $FFFFFE
VEC_RESET       EQU $000001

; VRAM layout
VRAM_PATTERN    EQU $FDFE00     ; Pattern Table
VRAM_BG0        EQU $FFC9E0     ; Nametable BG0
VRAM_BG1        EQU $FFD340     ; Nametable BG1
VRAM_BG2        EQU $FFDCA0     ; Nametable BG2
VRAM_PALETTE    EQU $FFE600     ; Palette RAM
VRAM_FONT       EQU $FFF600     ; Font Table
SCREEN_W        EQU 320
SCREEN_H        EQU 240

; Video I/O
V_CTRL          EQU $FFFF80
V_LINE          EQU $FFFF81
V_STAT          EQU $FFFF82
V_IRQLINE       EQU $FFFF83
V_SCROLLX0      EQU $FFFF84
V_SCROLLY0      EQU $FFFF86
V_SCROLLX1      EQU $FFFF88
V_SCROLLY1      EQU $FFFF8A
V_BANK          EQU $FFFF8C
V_BRIGHT        EQU $FFFF8D
V_IRQEN         EQU $FFFF8E
V_PALGRP        EQU $FFFF8F
V_IRQST         EQU $FFFF90

; Sprite OAM
SPR_OAM     EQU $FDF600     ; OAM base, 256 sprites x 8 bytes
SPR_COUNT   EQU 256            ; Number of sprites
SPR_SIZE    EQU 8              ; Bytes per OAM entry

; Byte offsets within each OAM entry
SPR_X       EQU 0
SPR_Y       EQU 1
SPR_TILE    EQU 2
SPR_ATTR    EQU 3
SPR_PAL     EQU 4
SPR_XH      EQU 5
SPR_EXT     EQU 6

; Attribute byte bit masks
SPR_PRIO    EQU %10000000   ; 1 = behind BG0 priority tiles
SPR_FLIPH   EQU %01000000
SPR_FLIPV   EQU %00100000
SPR_EXTSZ   EQU %00000001   ; byte SPR_EXT: 1 = size table B
SPR_SZ_8    EQU %00000000   ; 8x8
SPR_SZ_816  EQU %00001000   ; 8x16
SPR_SZ_16   EQU %00010000   ; 16x16
SPR_SZ_32   EQU %00011000   ; 32x32
SPR_XSZ_168 EQU %00000000   ; 16x8  (also set SPR_EXTSZ in byte SPR_EXT)
SPR_XSZ_24  EQU %00001000   ; 24x24  (also set SPR_EXTSZ in byte SPR_EXT)

; Keyboard / gamepad
KB_CTRL         EQU $FFFFC0
KB_STAT         EQU $FFFFC1
KB_SCAN         EQU $FFFFC2
KB_DATA         EQU $FFFFC3
KB_MOD          EQU $FFFFC4
KB_ROW          EQU $FFFFC5
KB_COL          EQU $FFFFC6
KB_FIFO         EQU $FFFFC7
KB_FIFOSTAT     EQU $FFFFC8
KB_REP          EQU $FFFFC9
KB_DELAY        EQU $FFFFCA
KB_RATE         EQU $FFFFCB
KB_LED          EQU $FFFFCC
KB_CMD          EQU $FFFFCD
KB_MATRIX       EQU $FFFFCE
KB_PAD          EQU $FFFFDC     ; gamepad buttons (matrix row 7, low byte)
BTN_A       EQU %10000000
BTN_B       EQU %01000000
BTN_START   EQU %00100000
BTN_SELECT  EQU %00010000
BTN_UP      EQU %00001000
BTN_DOWN    EQU %00000100
BTN_LEFT    EQU %00000010
BTN_RIGHT   EQU %00000001

; Audio
AUD_BASE        EQU $FFFF00
AUD_STRIDE      EQU 8     ; bytes per voice
AUD_VOICES      EQU 8
AV_VOL_L        EQU 0     ; offset inside a voice
AV_VOL_R        EQU 1     ; offset inside a voice
AV_PITCH_L      EQU 2     ; offset inside a voice
AV_PITCH_H      EQU 3     ; offset inside a voice
AV_SRC          EQU 4     ; offset inside a voice
AV_ADSR1        EQU 5     ; offset inside a voice
AV_ADSR2        EQU 6     ; offset inside a voice
AV_CTRL         EQU 7     ; offset inside a voice
AUD_V0_VOL_L    EQU $FFFF00
AUD_V0_VOL_R    EQU $FFFF01
AUD_V0_PITCH_L  EQU $FFFF02
AUD_V0_PITCH_H  EQU $FFFF03
AUD_V0_SRC      EQU $FFFF04
AUD_V0_ADSR1    EQU $FFFF05
AUD_V0_ADSR2    EQU $FFFF06
AUD_V0_CTRL     EQU $FFFF07
AUD_V1_VOL_L    EQU $FFFF08
AUD_V1_VOL_R    EQU $FFFF09
AUD_V1_PITCH_L  EQU $FFFF0A
AUD_V1_PITCH_H  EQU $FFFF0B
AUD_V1_SRC      EQU $FFFF0C
AUD_V1_ADSR1    EQU $FFFF0D
AUD_V1_ADSR2    EQU $FFFF0E
AUD_V1_CTRL     EQU $FFFF0F
AUD_V2_VOL_L    EQU $FFFF10
AUD_V2_VOL_R    EQU $FFFF11
AUD_V2_PITCH_L  EQU $FFFF12
AUD_V2_PITCH_H  EQU $FFFF13
AUD_V2_SRC      EQU $FFFF14
AUD_V2_ADSR1    EQU $FFFF15
AUD_V2_ADSR2    EQU $FFFF16
AUD_V2_CTRL     EQU $FFFF17
AUD_V3_VOL_L    EQU $FFFF18
AUD_V3_VOL_R    EQU $FFFF19
AUD_V3_PITCH_L  EQU $FFFF1A
AUD_V3_PITCH_H  EQU $FFFF1B
AUD_V3_SRC      EQU $FFFF1C
AUD_V3_ADSR1    EQU $FFFF1D
AUD_V3_ADSR2    EQU $FFFF1E
AUD_V3_CTRL     EQU $FFFF1F
AUD_V4_VOL_L    EQU $FFFF20
AUD_V4_VOL_R    EQU $FFFF21
AUD_V4_PITCH_L  EQU $FFFF22
AUD_V4_PITCH_H  EQU $FFFF23
AUD_V4_SRC      EQU $FFFF24
AUD_V4_ADSR1    EQU $FFFF25
AUD_V4_ADSR2    EQU $FFFF26
AUD_V4_CTRL     EQU $FFFF27
AUD_V5_VOL_L    EQU $FFFF28
AUD_V5_VOL_R    EQU $FFFF29
AUD_V5_PITCH_L  EQU $FFFF2A
AUD_V5_PITCH_H  EQU $FFFF2B
AUD_V5_SRC      EQU $FFFF2C
AUD_V5_ADSR1    EQU $FFFF2D
AUD_V5_ADSR2    EQU $FFFF2E
AUD_V5_CTRL     EQU $FFFF2F
AUD_V6_VOL_L    EQU $FFFF30
AUD_V6_VOL_R    EQU $FFFF31
AUD_V6_PITCH_L  EQU $FFFF32
AUD_V6_PITCH_H  EQU $FFFF33
AUD_V6_SRC      EQU $FFFF34
AUD_V6_ADSR1    EQU $FFFF35
AUD_V6_ADSR2    EQU $FFFF36
AUD_V6_CTRL     EQU $FFFF37
AUD_V7_VOL_L    EQU $FFFF38
AUD_V7_VOL_R    EQU $FFFF39
AUD_V7_PITCH_L  EQU $FFFF3A
AUD_V7_PITCH_H  EQU $FFFF3B
AUD_V7_SRC      EQU $FFFF3C
AUD_V7_ADSR1    EQU $FFFF3D
AUD_V7_ADSR2    EQU $FFFF3E
AUD_V7_CTRL     EQU $FFFF3F
AUD_MVOL_L      EQU $FFFF40
AUD_MVOL_R      EQU $FFFF41
AUD_KON         EQU $FFFF42
AUD_KOFF        EQU $FFFF43
AUD_STAT        EQU $FFFF44
AUD_ENDX        EQU $FFFF45
AUD_IRQEN       EQU $FFFF46
AUD_DIR         EQU $FFFF47
AUD_NOISE       EQU $FFFF48
AUD_ACTRL       EQU $FFFF49
AUD_ENVX        EQU $FFFF50
AUD_PITCH_1_0   EQU $1000     ; native-rate pitch

; Background tile attributes (byte 1 of a tile entry)
BG0_PRIORITY    EQU %00000001
BG0_FLIP_HORIZONTAL EQU %00000010
BG0_FLIP_VERTICAL EQU %00000100
BG0_PALETTE     EQU %01111000
BG0_PALETTE_SHIFT EQU 3
BG1_FLIP_HORIZONTAL EQU %00000001
BG1_FLIP_VERTICAL EQU %00000010
BG1_PALETTE     EQU %00111100
BG1_PALETTE_SHIFT EQU 2

; Boot ROM and cartridge
BOOT_ROM        EQU $FBB600     ; reset entry point
FIXED_ENTRY     EQU $000100     ; cartridge image load address and entry
CART_WIN        EQU $FBF600     ; 64 KB banked window
CART_WIN_END    EQU $FCF5FF
CART_STAT       EQU $FFFEF0
CART_BANK       EQU $FFFEF1
CART_CTRL       EQU $FFFEF2
CART_SBANK      EQU $FFFEF3
CART_BANK_SIZE  EQU 65536     ; bytes per ROM bank
CH_LEN          EQU 64     ; cartridge header size
CH_MAGIC        EQU 0     ; header offset, 4 byte(s)
CH_VER_MAJOR    EQU 4     ; header offset, 1 byte(s)
CH_VER_MINOR    EQU 5     ; header offset, 1 byte(s)
CH_FLAGS        EQU 6     ; header offset, 2 byte(s)
CH_MAPPER       EQU 8     ; header offset, 1 byte(s)
CH_HDR_SIZE     EQU 9     ; header offset, 1 byte(s)
CH_SAVE_KB      EQU 10     ; header offset, 2 byte(s)
CH_IMAGE_LEN    EQU 12     ; header offset, 3 byte(s)
CH_ENTRY        EQU 15     ; header offset, 3 byte(s)
CH_CHECKSUM     EQU 18     ; header offset, 4 byte(s)
CH_VEC_SWI      EQU 22     ; header offset, 3 byte(s)
CH_VEC_NMI      EQU 25     ; header offset, 3 byte(s)
CH_VEC_IRQ      EQU 28     ; header offset, 3 byte(s)
CH_VEC_FIRQ     EQU 31     ; header offset, 2 byte(s)
CH_TITLE        EQU 33     ; header offset, 16 byte(s)
CH_RESERVED     EQU 49     ; header offset, 15 byte(s)
MAPPER_ROM      EQU 0
MAPPER_ROM_SAVE EQU 1
ERR_MAGIC       EQU 1     ; Cartridge magic is not P63C
ERR_MAJOR       EQU 2     ; Unsupported major version
ERR_MINOR       EQU 3     ; Unsupported minor version
ERR_FLAGS       EQU 4     ; Header flags bits set (v1.0 requires zero)
ERR_MAPPER      EQU 5     ; Unsupported mapper
ERR_HEADER      EQU 6     ; Header size or reserved bytes invalid
ERR_ENTRY       EQU 7     ; Header entry address is not the fixed entry
ERR_CHECKSUM    EQU 8     ; Image checksum mismatch
ERR_RAM         EQU 9     ; RAM failed POST
ERR_RETURNED    EQU 10     ; Cartridge entry returned
ERR_INTERRUPT   EQU 11     ; Unexpected interrupt or SWI
ERR_LENGTH      EQU 12     ; Image length out of range
ERR_NOCART      EQU 13     ; No cartridge inserted
