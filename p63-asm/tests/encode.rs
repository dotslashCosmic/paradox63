//! Single lines assembled to exact bytes.

use p63_asm::assemble;

fn bytes(line: &str) -> Vec<u8> {
    let src = format!("    ORG $1000\nHERE:\n    {line}\n");
    match assemble("test", &src) {
        Ok(p) => p.segments.into_iter().flat_map(|(_, b)| b).collect(),
        Err(e) => panic!("`{line}`: {}", e[0]),
    }
}

fn error(src: &str) -> String {
    match assemble("test", src) {
        Ok(_) => panic!("expected an error for {src:?}"),
        Err(e) => e[0].to_string(),
    }
}

#[test]
fn loads_and_stores() {
    assert_eq!(bytes("LD A,#$42"), [0x01, 0x42]);
    assert_eq!(bytes("LD D,#$1234"), [0x03, 0x34, 0x12]);
    assert_eq!(bytes("LD Q,#$12345678"), [0x07, 0x78, 0x56, 0x34, 0x12]);
    assert_eq!(bytes("LD A,<$25"), [0x46, 0x25]);
    assert_eq!(bytes("ST W,<$10"), [0x0E, 0x10]);
    assert_eq!(bytes("LD A,$FEDCBA"), [0xC0, 0xBA, 0xDC, 0xFE]);
    assert_eq!(bytes("ST Q,$002000"), [0xCD, 0x00, 0x20, 0x00]);
    assert_eq!(bytes("LD X,$002000"), [0xCE, 0x00, 0x20, 0x00]);
    assert_eq!(bytes("LD X,#$123456"), [0x5A, 0x08, 0x56, 0x34, 0x12]);
    assert_eq!(bytes("MOV GP,#16"), [0x5A, 0x07, 0x10]);
    assert_eq!(bytes("MOV D,A"), [0x08, 0x02]);
}

#[test]
fn indexed_picks_the_smallest_form() {
    assert_eq!(bytes("LD A,0,X"), [0xD0, 0x00]);
    assert_eq!(bytes("LD A,2,X"), [0xD0, 0x42]);
    assert_eq!(bytes("LD A,-10,Y"), [0xD0, 0xF6]);
    assert_eq!(bytes("LD A,127,X"), [0xD0, 0x01, 0x7F]);
    assert_eq!(bytes("LD A,1000,X"), [0xD0, 0x02, 0xE8, 0x03]);
    assert_eq!(bytes("LD A,$10000,Y"), [0xD0, 0x83, 0x00, 0x00, 0x01]);
    assert_eq!(bytes("LEA X,1,X"), [0x54, 0x41]);
    assert_eq!(bytes("LEA X,-256,X"), [0x54, 0x02, 0x00, 0xFF]);
}

#[test]
fn alu_forms() {
    assert_eq!(bytes("ADD A,B"), [0x10, 0x10]);
    assert_eq!(bytes("ADD D,W"), [0x10, 0x52]);
    assert_eq!(bytes("CMP A,#5"), [0x1B, 0xE0, 0x05]);
    assert_eq!(bytes("CMP X,#$1000"), [0x1B, 0xE8, 0x00, 0x10, 0x00]);
    assert_eq!(bytes("CMP A,#-1"), [0x1B, 0xE0, 0xFF]);
    assert_eq!(bytes("ADD GP,<$20"), [0x10, 0xF7, 0x00, 0x20]);
    assert_eq!(bytes("SUB D,$012345"), [0x12, 0xF2, 0x01, 0x45, 0x23, 0x01]);
    assert_eq!(bytes("OR A,3,X"), [0x15, 0xF0, 0x02, 0x43]);
    assert_eq!(bytes("BIT E,#$80"), [0x29, 0xE3, 0x80]);
    assert_eq!(bytes("INC A"), [0x18, 0x00]);
    assert_eq!(bytes("INC A,#5"), [0x18, 0xE0, 0x05]);
    assert_eq!(bytes("LSR Q,#17"), [0x23, 0xE6, 0x11]);
    assert_eq!(bytes("TST D"), [0x28, 0x02]);
    assert_eq!(bytes("DIV D,E,S"), [0x1D, 0x01]);
    assert_eq!(bytes("ZEXT W,Q"), [0x2D, 0x56]);
    assert_eq!(bytes("SMODX X,#-1"), [0x55, 0xE8, 0xFF, 0xFF, 0xFF]);
}

#[test]
fn branches_and_calls() {
    assert_eq!(bytes("BRA HERE"), [0x30, 0xFE]);
    assert_eq!(bytes("BEQ *+10"), [0x32, 0x08]);
    assert_eq!(bytes("LBRA HERE"), [0x45, 0xFD, 0xFF]);
    assert_eq!(bytes("JSR $002000"), [0x41, 0x00, 0x20, 0x00]);
    assert_eq!(bytes("BSR HERE"), [0x44, 0xFE]);
    assert_eq!(bytes("DBNZ B,HERE"), [0x96, 0x01, 0xFD]);
    assert_eq!(bytes("TBZ A,#6,HERE"), [0x97, 0x06, 0xFD]);
    assert_eq!(bytes("RTS"), [0x42, 0x00]);
}

#[test]
fn far_branches_become_long() {
    let p = assemble("t", "    ORG $1000\n    BEQ FAR\n    .DS 300\nFAR: RTS\n").unwrap();
    let code = &p.segments[0].1;
    assert_eq!(&code[..3], &[0x80, 0x2C, 0x01]); // LBEQ +300
    assert_eq!(p.symbol("FAR"), Some(0x1000 + 3 + 300));
}

#[test]
fn bit_block_and_special() {
    assert_eq!(bytes("BTST E,#3"), [0x93, 0x33]);
    assert_eq!(bytes("BSET $2000,#5"), [0x90, 0xF5, 0x01, 0x00, 0x20, 0x00]);
    assert_eq!(bytes("PACK D,A,B"), [0x50, 0x01, 0x02]);
    assert_eq!(bytes("UNP B,A,D"), [0x51, 0x10, 0x02]);
    assert_eq!(bytes("BEXT A,D,#4,#3"), [0x52, 0x20, 0x42]);
    assert_eq!(bytes("BINS A,GP,#5,#3"), [0x53, 0x70, 0x52]);
    assert_eq!(bytes("MOVB [X],[Y],#32"), [0x60, 0x20]);
    assert_eq!(bytes("MOVW [X],[Y],#1000"), [0x61, 0x80, 0xE8, 0x03]);
    assert_eq!(bytes("FILL [X],#$ABCD,#500"), [0x62, 0x80, 0xF4, 0x01, 0xCD, 0xAB]);
    assert_eq!(bytes("DAA"), [0x5D, 0x00]);
}

#[test]
fn data_directives_and_locals() {
    let src = "    ORG $2000\nTAB: .BYTE 1,$FF,\"AB\"\n    .WORD $1234\n    .ADDR TAB\n    .LONG -1\nN EQU TAB+2\nF1:\n.L: BRA .L\nF2:\n.L: BRA .L\n";
    let p = assemble("t", src).unwrap();
    assert_eq!(
        p.segments[0].1[..14],
        [1, 0xFF, b'A', b'B', 0x34, 0x12, 0x00, 0x20, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0x30]
    );
    assert_eq!(p.symbol("N"), Some(0x2002));
    assert_ne!(p.symbol("F1.L"), p.symbol("F2.L"), "locals are scoped to their label");
}

#[test]
fn errors_name_the_line() {
    assert_eq!(error("    MUL A,A\n"), "test:1: MUL only works as `MUL A,B[,S]`");
    assert_eq!(error("\n    JMP NOWHERE\n"), "test:2: undefined symbol `NOWHERE`");
    assert_eq!(error("    LD A,#300\n"), "test:1: immediate 300 doesn't fit in 8 bits");
    assert_eq!(error("    ST GP,$2000\n"), "test:1: ST only stores A B D E F W Q, not GP");
    assert!(error("X: NOP\nX: NOP\n").contains("already defined"));
}
