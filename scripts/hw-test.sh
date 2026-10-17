#!/usr/bin/env bash
# Hardware regression on any chip with a flash driver (bench: STM32H743 +
# ST-Link V2 clone). The chip, flash size and family come from `probe`;
# H7-only checks (DBGMCU through AP2) run only on H7.
#
#   scripts/hw-test.sh                 # non-destructive checks only
#   scripts/hw-test.sh firmware.bin    # also flash firmware.bin and verify it
#   scripts/hw-test.sh --erase-all [firmware.bin]
#                                      # also erase the entire flash (destructive;
#                                      # firmware.bin, if given, is written back after)
#
# Reference tools (st-info, st-flash) are used when installed.
# The target is reset several times (connect under reset).

set -uo pipefail

cd "$(dirname "$0")/.."
ERASE_ALL=0
if [[ "${1:-}" == "--erase-all" ]]; then
    ERASE_ALL=1
    shift
fi
FW="${1:-}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cargo build -q -p stlink-cli || exit 1
CLI=(./target/debug/stlink-cli)
R=(--connect-under-reset)

# What is connected (connect under reset also wakes sleeping firmware).
PROBE="$("${CLI[@]}" "${R[@]}" probe 2>/dev/null)"
CHIPID="$(echo "$PROBE" | sed -n 's/^  chipid: *//p')"
FLASH="$(echo "$PROBE" | sed -n 's/^  flash: *\([0-9]*\).*/\1/p')"
FTYPE="$(echo "$PROBE" | sed -n 's/^  flash-type: *//p')"
if [[ -z "$CHIPID" || -z "$FLASH" || "$FLASH" -eq 0 ]]; then
    echo "no target found:"
    echo "$PROBE"
    exit 1
fi
echo "target: chip id $CHIPID, flash $FLASH bytes, flash type ${FTYPE:-unknown}"
echo "$PROBE" | grep -E '^  layout:'
# Read size for the comparison checks: 128 KiB or the whole flash if smaller.
RSIZE=$((FLASH < 0x20000 ? FLASH : 0x20000))

pass=0
fail=0
check() { # name, command...
    local name="$1"
    shift
    if out="$("$@" 2>&1)"; then
        echo "PASS  $name"
        pass=$((pass + 1))
    else
        echo "FAIL  $name"
        echo "$out" | sed 's/^/      /' | tail -8
        fail=$((fail + 1))
    fi
}
has() { command -v "$1" >/dev/null; }

check "list finds an ST-Link" \
    bash -c "'${CLI[*]}' list | grep -q '0483:'"

# First: firmware that sleeps with debug clocks off (the bench) answers
# erratically until DBGMCU keeps debug alive; this lasts until power-off.
check "keep-debug-in-low-power, then plain probe works" \
    bash -c "'${CLI[*]}' --keep-debug-in-low-power probe >/dev/null && '${CLI[*]}' probe | grep -q 'chipid:     $CHIPID'"

if has st-info; then
    # Sequential, not <(...): two programs cannot share the probe.
    fields='^  (version|serial|flash|sram|chipid|dev-type):'
    check "probe matches st-info --probe" bash -c "
        '${CLI[*]}' ${R[*]} probe 2>/dev/null | grep -E '$fields' >'$TMP/ours.txt'
        st-info --probe --connect-under-reset 2>/dev/null | grep -E '$fields' >'$TMP/ref.txt'
        diff '$TMP/ref.txt' '$TMP/ours.txt'"
fi

# Also resumes the core: st-info leaves it halted at the reset vector.
check "probe (under reset) finds a known chip" \
    bash -c "'${CLI[*]}' ${R[*]} probe | grep -qE 'dev-type: +STM32|dev-type: +F'"

if [[ "$FTYPE" == "H7" ]]; then
    check "DBGMCU via AP2 reads the H7 IDCODE" \
        bash -c "'${CLI[*]}' mem --ap 2 0xE00E1000 | grep -qi '${CHIPID#0x}$'"
fi


check "regs: PC in flash, core resumed" bash -c "
    out=\$('${CLI[*]}' regs 2>&1)
    echo \"\$out\" | grep -qE 'pc +0x08' && echo \"\$out\" | grep -q resumed || { echo \"\$out\"; false; }"

check "read $RSIZE bytes at 1.8 and 4 MHz match" bash -c "
    '${CLI[*]}' ${R[*]} read '$TMP/a.bin' --size $RSIZE >/dev/null &&
    '${CLI[*]}' ${R[*]} --freq 4000 read '$TMP/b.bin' --size $RSIZE >/dev/null &&
    cmp -s '$TMP/a.bin' '$TMP/b.bin'"

if has st-flash; then
    check "read matches st-flash read" bash -c "
        st-flash --connect-under-reset read '$TMP/ref.bin' 0x08000000 $RSIZE >/dev/null 2>&1 &&
        cmp -s '$TMP/a.bin' '$TMP/ref.bin'"
fi

head -c 64 /dev/zero >"$TMP/z.bin"
# Unaligned for the family's write unit (F0/F1: halfwords, F2/F4/F7: words
# at x32, G4: double words, H7: 32-byte flash words). Not on F2/F4/F7: below
# 2.7 V they program bytes and the address would be valid.
case "$FTYPE" in
    H7) UNALIGNED=0x08000010 ;;
    G4) UNALIGNED=0x08000004 ;;
    F0_F1_F3 | F1_XL) UNALIGNED=0x08000001 ;;
    *) UNALIGNED="" ;;
esac
if [[ -n "$UNALIGNED" ]]; then
    check "write rejects an unaligned address (nothing erased)" \
        bash -c "! '${CLI[*]}' ${R[*]} write '$TMP/z.bin' --addr $UNALIGNED 2>&1 | grep -q Erasing"
fi
head -c $((FLASH + 1024)) /dev/zero >"$TMP/big.bin"
check "write rejects a file larger than flash" \
    bash -c "! '${CLI[*]}' ${R[*]} write '$TMP/big.bin'"
check "write rejects non-.bin files" \
    bash -c "! '${CLI[*]}' write '$TMP/z.hex'"
check "flash unchanged after rejected writes" bash -c "
    '${CLI[*]}' ${R[*]} read '$TMP/c.bin' --size $RSIZE >/dev/null && cmp -s '$TMP/a.bin' '$TMP/c.bin'"

if [[ $ERASE_ALL -eq 1 ]]; then
    check "erase-all erases every bank" \
        bash -c "'${CLI[*]}' ${R[*]} erase-all | grep -q 'flash erased in'"
    check "whole flash reads as 0xFF after erase-all" bash -c "
        '${CLI[*]}' ${R[*]} --freq 4000 read '$TMP/erased.bin' >/dev/null &&
        size=\$(wc -c <'$TMP/erased.bin' | tr -d ' ') &&
        [[ \$size -gt 0 ]] &&
        cmp -s '$TMP/erased.bin' <(head -c \$size /dev/zero | tr '\\0' '\\377')"
fi

if [[ -n "$FW" ]]; then
    check "write $FW with verify and reset" \
        bash -c "'${CLI[*]}' ${R[*]} --freq 4000 write '$FW' | grep -q 'firmware running'"
    check "independent read-back equals the file" bash -c "
        size=\$(wc -c <'$FW' | tr -d ' ') &&
        '${CLI[*]}' ${R[*]} read '$TMP/fw.bin' --size \$size >/dev/null &&
        cmp -s '$FW' '$TMP/fw.bin'"
fi

echo "---"
echo "$pass passed, $fail failed"
[[ $fail -eq 0 ]]
