import subprocess, sys, shutil, os
S=sys.argv[1]
muts=[
 ("spu2/mixer.rs","volume.wrapping_mul(value) >> 15","volume.wrapping_mul(value) >> 14"),
 ("spu2/reverb.rs","volume.wrapping_mul(value) >> 15","volume.saturating_mul(value) >> 15"),
 ("driver/iop_clock.rs","const MULTIPLY_EXTRA: u32 = 7;","const MULTIPLY_EXTRA: u32 = 6;"),
 ("driver/sequencer.rs","let pan = 0.max(0x7f.min(s16(i64::from(mem.u8(chan + 4)?)","let pan = 0.max(0x7e.min(s16(i64::from(mem.u8(chan + 4)?)"),
 ("spu2/envelope.rs","slowdown += 2;","slowdown += 1;"),
 ("spu2/gauss.rs","out += (i32::from(gauss_tap(i)) * i32::from(samples[3])) >> 15;","out += (i32::from(gauss_tap(i)) * i32::from(samples[3])) >> 14;"),
 ("live_audio.rs","0x6150 => 23,","0x6150 => 22,"),
 ("ee_sound_queue.rs","if slot.id == DEDUP_ID && seen && previous == slot.a2 {","if slot.id == DEDUP_ID && previous == slot.a2 {"),
 ("driver/libsd.rs","let wide = u32::from(value) << 2;","let wide = u32::from(value) << 1;"),
 ("spu2/spu2.rs","4 => v.next_address = ((value & 0x0F) << 16) | (v.next_address & 0xFFF8) | 1,","4 => v.next_address = ((value & 0x0F) << 16) | (v.next_address & 0xFFF8),"),
]
for f,old,new in muts:
    src=open(f).read()
    assert src.count(old)==1, (f,old,src.count(old))
    shutil.copy(f, S+'/backup.rs')
    open(f,'w').write(src.replace(old,new))
    try:
        r=subprocess.run(["cargo","test","-p","audio","--test","golden_sound","--test","units","--test","system","-q"],capture_output=True,text=True,timeout=900)
        failed=[l.split()[1] for l in r.stdout.splitlines() if l.startswith("test ") and l.endswith("FAILED")]
        print(f"{f}: {old[:40]!r} -> killed by {failed if failed else 'NOTHING'} (exit {r.returncode})", flush=True)
    finally:
        shutil.copy(S+'/backup.rs', f)
