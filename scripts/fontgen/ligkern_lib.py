import struct, subprocess
_UNUSED_FONTS = ['cmr10', 'cmr7', 'cmr5', 'cmbx10', 'cmti10', 'cmmi10', 'cmmi7', 'cmmi5', 'cmss10', 'cmtt10']
def ligkern(name, fix_words=False):
    """Kerns (1/10000 em, or TFM fix_words with `fix_words`) and ligatures by pair key."""
    path = subprocess.check_output(['kpsewhich', name + '.tfm']).decode().strip()
    d = open(path, 'rb').read()
    lf, lh, bc, ec, nw, nh, nd, ni, nl, nk, ne, np_ = struct.unpack('>12H', d[:24])
    ci = 24 + lh * 4
    wb = ci + (ec - bc + 1) * 4
    lkb = wb + (nw + nh + nd + ni) * 4
    kb = lkb + nl * 4
    fix = lambda off: struct.unpack('>i', d[off:off + 4])[0] / 2**20
    lk = [d[lkb + 4 * i: lkb + 4 * i + 4] for i in range(nl)]
    kerns, ligs = {}, {}
    for c in range(bc, ec + 1):
        b0, b1, b2, b3 = d[ci + (c - bc) * 4: ci + (c - bc) * 4 + 4]
        if b0 == 0 or (b2 & 3) != 1: continue
        i = b3
        if lk[i][0] > 128: i = 256 * lk[i][2] + lk[i][3]
        while True:
            skip, nxt, op, rem = lk[i]
            if skip <= 128:
                key = (c << 8) | nxt
                if key not in kerns and key not in ligs:
                    if op >= 128:
                        off = kb + 4 * (256 * (op - 128) + rem)
                        kerns[key] = struct.unpack('>i', d[off:off + 4])[0] if fix_words else round(fix(off) * 10000)
                    else:
                        ligs[key] = (op, rem)
            if skip >= 128: break
            i += skip + 1
    return kerns, ligs
