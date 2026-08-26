# Windows display-audit checkpoint — 2026-08-26

This is a bounded local assay, not a durable or independently custodied proof.
It records why the display path was changed and preserves the failed controls
instead of folding them into the final pass.

## Instrument

With `BITBLOCK_DISPLAY_AUDIT=1`, Bitblock captures the exact Win32 client
rectangle from the GDI screen DC into a top-down 32-bit `BI_RGB` DIB using
`SRCCOPY | CAPTUREBLT`. It checks every API result, requires one-monitor stable
physical geometry and DPI, normalizes only the unused high byte, observes the
complete captured frame, and compares every XRGB word before interpreting hash
agreement. `GdiFlush`, `DwmFlush`, the capture interval, geometry, DPI, capture
facts, and mismatch witnesses enter an append-only display-receipt chain.

## Powered history

1. **Rounded-corner failure.** The first live 801 x 801 sample returned a
   complete capture with 36 mismatches, first index `635994 = (0, 794)`. The
   failures were symmetric at the bottom corners:

   ```text
   y=794: x=0,800
   y=795: x=0,800
   y=796: x=0,800
   y=797: x=0,1,799,800
   y=798: x=0,1,799,800
   y=799: x=0,1,2,3,797,798,799,800
   y=800: x=0..6,794..800
   ```

   Expected words were the bedrock-front `0x00e8890c`; captured words were
   external desktop/window colors. Receipt:
   `#bb.display-receipt.sha256.vd4jwmdhxn3kpuwnbmy2sv3hbxdmqy5tomsmd4frscv3nvwtgpuq`.
   This demonstrated that the instrument read the visible desktop raster rather
   than rereading Bitblock's CPU buffer. DWM corner rounding was then disabled.

2. **Composition-race failure.** Immediately sampling the first square-corner
   window observed the prior desktop at all `641601` pixels, first index zero.
   Receipt:
   `#bb.display-receipt.sha256.2jqfcelsz6ofvrn52qxxe4k3djd2dq7vgquyzjeesf34kcamxgba`.
   This showed that checked GDI submission alone did not settle compositor
   timing. No retry loop was added; the failed occurrence remains part of the
   assay. A checked `DwmFlush` barrier was added to the capture policy.

3. **Full-slice pass.** Under the square-corner policy plus checked GDI and DWM
   barriers, the next live 801 x 801 capture returned `mismatches=0` and
   `first=None`. Receipt:
   `#bb.display-receipt.sha256.yr5ziyesmgyk7v5eizplfnhgc5u5237s4ikdntstzky4aqpanvka`.

4. **Full-client sidebar pass.** After adding the separately clipped sidebar,
   the submitted artifact became a 1201 x 801 composite containing the exact
   801 x 801 game frame at the left and a 400 x 801 metrics panel at the right.
   A console-enabled live run captured all `962001` pixels and returned
   `mismatches=0`, `first=None`. Receipt:
   `#bb.display-receipt.sha256.bfzf2andj6t7zun745rwm44lnybuy567cdnjn3j3mxcudvviy7fq`.

   An earlier GUI-subsystem release launch showed only `Mismatch / Complete` in
   its window title. That run emitted no recoverable console fields, so its
   receipt and first mismatch are unavailable and no mechanism is attributed.
   It is retained as an unresolved title-only observation, not counted as the
   full-client pass and not retried away inside one display ledger.

5. **Cascaded-position bottom-strip failure.** A later full-client launch
   returned a complete 1201 x 801 capture with `40834` mismatches, first index
   `921167 = (0, 767)`. The mismatch began as one horizontal boundary and
   continued through the bottom of the client; the first 767 rows matched.
   Receipt:
   `#bb.display-receipt.sha256.3and7uc7iw6uj6jipymrgslg77iwr4vuxvkiwajzi3gd32akxe3a`.
   Windows had cascaded the decorated window downward between launches. The app
   was changed only to horizontally center and top-align its outer window on
   the selected monitor before its first presentation.

6. **Top-aligned full-client pass.** With the same 1201 x 801 framebuffer and
   capture policy after that one placement change, the next live sample
   returned `mismatches=0`, `first=None`. Receipt:
   `#bb.display-receipt.sha256.zlvanxacsraxglop7ssdnyqduqzaed7gts2he73szpfgikeubmda`.
   This paired failure/pass identifies initial cascading as the mechanism for
   the measured bottom-strip loss on this display setup; it does not establish
   a universal placement guarantee for every monitor/work-area arrangement.

7. **Overlapping-window control and current pass.** During the Edit/storage/
   Ladder checkpoint, another Bitblock window remained above the newly launched
   diagnostic window. The complete 1201 x 801 capture rejected `8298` pixels,
   first index `54856 = (811,45)`: expected sidebar text yellow `0xf2c14e`,
   while the screen exposed the covering window's sidebar colors. Receipt:
   `#bb.display-receipt.sha256.vql2qeo7f635tnwklohss4ntzogt7lroot7zdusrugi3tywnwp6q`.

   After minimizing the exact covering handle and foregrounding the audited
   handle, an F3 sidebar recomposition under identical code returned
   `mismatches=0`, `first=None`. Receipt:
   `#bb.display-receipt.sha256.ai3tye7ysdysytl732vwjwey5a4drhe4ps5fnneghygxf3tvvibgq`.
   The feature-gated release artifact separately showed `Match / Complete` in
   its GUI title after the same foreground control. Its GUI subsystem exposes
   no console receipt, so the exact receipt claim here belongs to the diagnostic
   build. This pair demonstrates that overlap is observable rather than silently
   accepted; it does not prove an unoccluded window at every future sample.

8. **Checked Win32 presenter migration.** The pinned Softbuffer 0.4.8 Win32
   backend called `BitBlt` but discarded its Boolean return before reporting
   `Ok(())`. Bitblock replaced only the Windows submission layer with an owned
   top-down XRGB buffer and `SetDIBitsToDevice`; a frame occurrence is admitted
   only when the API returns the complete requested scanline count. The policy
   identity is
   `#bb.policy.sha256.opgpybj32tw72pep2xusit4lzzgsiozs5f5reppq67iiejueetvq`.

   The first new-backend run retained a partial external-raster failure with
   `516522` mismatches, first index `565 = (565,0)`, including dark window-chrome
   colors. Receipt:
   `#bb.display-receipt.sha256.2mhxy7s2aupoia3m55mnfxdu2jzathkpg4qlqafswxbveyyzwupa`.

   A second run first observed an entirely different external raster at all
   `962001` positions, first index zero. Receipt:
   `#bb.display-receipt.sha256.tx7yh6r7drcuwd7ku4cv2i33jvhstalop7da6ieu2y7whe4guxfa`.
   The same uninterrupted process subsequently emitted complete full-client
   passes. The first retained pass had `mismatches=0`, `first=None`:
   `#bb.display-receipt.sha256.2swl7ga2lqxiswavxqt4mznrxku4u7hcqfsxccvpjytt2hwfz2taq`.
   A later occurrence while the window was minimized returned the explicit
   `Minimized` unavailable stage:
   `#bb.display-receipt.sha256.vtuwt3n3fnkbvv6zbmj7xpced4pgnbkm5gthxxkrgw3ihbk5lrwqq`.

   This failure/pass sequence proves that the new checked call can reach the
   exact OS-exposed client raster and that external visibility still changes
   the observation. It does not identify whether the two failed occurrences
   were caused by occlusion, window visibility, or compositor scheduling, and
   it does not promote GDI acceptance or screen-DC equality into scanout proof.

9. **Canonical composition-timing envelope.** The research-only
   `BITBLOCK_PRESENTATION_AUDIT=1` path now observes each checked presentation
   with QPC-bracketed null-HWND `DwmGetCompositionTimingInfo` calls before and
   after it, followed by separately bracketed `GdiFlush` and `DwmFlush` calls.
   Every raw field of `DWM_TIMING_INFO` except caller-owned `cbSize` enters a
   fixed 948-byte presentation artifact together with the exact composite
   `FrameReceiptV2` and checked presenter facts. The policy identity is
   `#bb.policy.sha256.t2zuewerksnmvhhjyaaplmlmfb2poptubqxpc6czwvmuwufpanrq`;
   the Rust/Python known-answer artifact is
   `#bb.presentation-artifact.sha256.sw2i52veohfmc3lqe74tzd72n6ppwxxshejxmazclwikgniqx2sq`.

   Observation failures are retained and make the derived timing relation
   unavailable; they do not roll back a successful checked pixel submission.
   Only presenter failure prevents the frame occurrence. A powered subject
   control removed the required GDI barrier verdict and made the dedicated
   test report `ComparableAdvanced` instead of `Unavailable`; restoring the
   production condition returned the test to green.

   One live HWND run then emitted:

   ```text
   #bb.presentation-artifact.sha256.pmvfnxmzcjnz5ag26wmpdkdhbpoiofv4grxmwigofifjxj57kgya
   progress=ComparableAdvanced attribution=Unknown qpf=true
   pre_hr=0 gdi_flush=true dwm_flush_hr=0 post_hr=0
   ```

   The session-owned diagnostic process was stopped with Ctrl+C after that
   occurrence. Its full 948-byte payload was not written to the typed journal,
   so the tag and selected terminal fields are retained here but the raw object
   is not independently replayable custody.

   Modern Windows requires a null HWND for the timing API, making its counters
   desktop-composition-session scoped. The encoded artifact therefore exposes
   only `Unknown` frame attribution. Session counter movement can never be
   rewritten as proof that this frame displayed or dropped.

The historical failures and passes arose in separate in-memory runs; their tags
commit to their exact local fields, but the captured artifacts were not durably
retained. They are evidence for this checkpoint, not replayable external
custody. The unretained title-only sample has still less evidentiary weight and
carries no receipt claim.

## Boundary

The pass means: at one sampled interval, the OS screen device exposed the same
canonical pixels over Bitblock's visible client rectangle. It does not prove
that every prior or later frame displayed, that the sample reached scanout,
that color-management or HDR hardware emitted identical values, or that a
physical panel emitted matching photons. Those remain separate instruments and
must report `N/A` until present.
