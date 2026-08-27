# Bitblock view metrics

This catalog owns observer/camera, aiming and visual edit interaction, viewport and zoom,
renderer/rasterization/visibility, raw input, observer collision, sidebar self-observation,
and visual-correctness metrics.

Two original camera candidates were moved to the collision-owned namespace during the initial
cross-catalog deduplication pass. Retired registry names are tombstoned rather than recycled.

It deliberately does not own general tick health, wall-clock timing, frame latency, rates,
language/edit-batch accounting, replay, formal-proof status, or generic hashes. Those belong in
[system.md](system.md). World topology, storage, information, and emergence metrics belong in
[world.md](world.md). Where one of those values is needed as a denominator or timestamp, this
catalog names the dependency instead of registering the derived metric again.

## Measurement contract

Exactness:

- **Exact**: exact over the stored discrete state, event counter, or framebuffer.
- **Float**: deterministic for the stored f64 values, but not a proof over mathematical reals.
- **Sampled**: evaluated only at explicitly selected pixels or states; never a universal claim.

Availability:

- **Current**: derivable from existing Bitblock state, buffers, or pure queries. A public accessor
  or sidebar formatter may still need to be added.
- **Instrument**: requires retaining an event/intermediate, adding a counter, enriching a query
  result, adding an owner/reference buffer, or keeping a previous snapshot.

Cost symbols:

- **N**: number of on bits.
- **P**: physical pixels in the simulation viewport.
- **Q**: candidate pixel samples traversed by triangle bounding boxes.
- **R**: number of explicitly selected reference pixels.
- **U**: number of distinct coordinates retained by a history set.

Costs describe one snapshot or the incremental hook needed to maintain the metric. They exclude
text formatting and the system-owned cost of scheduling or presenting the snapshot.

## Observer and camera

| Registry name | Metric | Precise definition | Exactness | Availability | Cost |
|---|---|---|---|---|---|
| #bb.metric.cam.observer-position | Observer position | Camera world-space position (x, y, z). | Float | Current | O(1) |
| #bb.metric.cam.observer-cell | Observer cell | Integer coordinate returned by coord_containing(observer position). | Exact | Current | O(1) |
| #bb.metric.cam.observer-subcell-offset | Observer subcell offset | Observer position minus the center of #bb.metric.cam.observer-cell. | Float | Current | O(1) |
| #bb.metric.cam.observer-cell-face-clearances | Observer cell-face clearances | Distances from the observer point to the six faces of #bb.metric.cam.observer-cell. | Float | Current | O(1) |
| #bb.metric.cam.yaw | Yaw | Wrapped horizontal camera angle in radians. | Float | Current | O(1) |
| #bb.metric.cam.pitch | Pitch | Clamped vertical camera angle in radians. | Float | Current | O(1) |
| #bb.metric.cam.roll-invariant | Roll invariant | Stored or derived roll; currently exactly zero by camera construction. | Exact | Current | O(1) |
| #bb.metric.cam.camera-basis | Camera basis | Forward, right, and up vectors used by projection and movement. | Float | Current | O(1) |
| #bb.metric.cam.basis-orthonormality-residual | Basis orthonormality residual | Maximum of the three unit-length errors and three absolute pairwise dot products in #bb.metric.cam.camera-basis. | Float | Current | O(1) |
| #bb.metric.cam.basis-handedness | Basis handedness | Sign and residual of the determinant of #bb.metric.cam.camera-basis against the intended handedness. | Float | Current | O(1) |
| #bb.metric.cam.center-ray | Center ray | Ray with origin #bb.metric.cam.observer-position and direction equal to the camera forward vector. | Float | Current | O(1) |
| #bb.metric.cam.observation-anchor | Observation anchor | World-space point kept under the center ray during dolly. | Float | Current | O(1) |
| #bb.metric.cam.anchor-cell | Anchor cell | Integer coordinate containing #bb.metric.cam.observation-anchor, or unrepresentable. | Exact | Current | O(1) |
| #bb.metric.cam.observation-distance | Observation distance | Stored distance between observer and observation anchor. | Float | Current | O(1) |
| #bb.metric.cam.anchor-range-residual | Anchor range residual | Absolute difference between geometric observer-anchor distance and #bb.metric.cam.observation-distance. | Float | Current | O(1) |
| #bb.metric.cam.anchor-direction-residual | Anchor direction residual | Angle between camera forward and normalized anchor-minus-observer. | Float | Current | O(1) |
| #bb.metric.cam.field-of-view | Field of view | Camera field of view in radians for a declared viewport axis; the vertical value is stored and the horizontal value is derived from it and the viewport aspect. | Float | Current | O(1) |
| #bb.metric.cam.focal-length | Focal length | Perspective focal length in physical pixels for the current viewport height. | Float | Current | O(1) |
| #bb.metric.cam.view-solid-angle | View solid angle | Solid angle subtended by the rectangular perspective view. | Float | Current | O(1) |
| #bb.metric.cam.anchor-plane-span | Anchor-plane span | World-space width and height visible on the plane through #bb.metric.cam.observation-anchor. | Float | Current | O(1) |
| #bb.metric.cam.unit-voxel-angular-span | Unit-voxel angular span | Angular width and height of a unit voxel centered at anchor depth. | Float | Current | O(1) |
| #bb.metric.cam.origin-distance | Origin distance | Euclidean distance from #bb.metric.cam.observer-position to the fixed bedrock-coordinate center. | Float | Current | O(1) |
| #bb.metric.cam.origin-bearing | Origin bearing | Yaw and pitch required to face the origin from #bb.metric.cam.observer-position. | Float | Current | O(1) |
| #bb.metric.cam.origin-look-error | Origin look error | Angular separation between camera forward and the vector from #bb.metric.cam.observer-position to the fixed bedrock-coordinate center. | Float | Current | O(1) |
| #bb.metric.cam.origin-projection | Origin projection | Projected physical-pixel coordinates and camera-space depth of the origin center. | Float | Current | O(1) |
| #bb.metric.cam.origin-viewport-state | Origin viewport state | Behind-camera, projected offscreen, or projected onscreen classification. | Exact | Current | O(1) |
| #bb.metric.cam.origin-line-of-sight | Origin line of sight | Whether the first occupied AABB on the observer-to-origin ray is bedrock. | Float | Current | O(N) |
| #bb.metric.cam.spawn-displacement | Spawn displacement | Vector and Euclidean distance from the observer spawn position. | Float | Current | O(1) |
| #bb.metric.cam.coordinate-ulp | Coordinate ULP | Representable f64 spacing near each observer-position component. | Float | Current | O(1) |
| #bb.metric.cam.cell-resolution-status | Cell-resolution status | Whether #bb.metric.cam.coordinate-ulp remains below one block, half a block, and configured collision clearance. | Exact | Current | O(1) |
| #bb.metric.cam.camera-finite-invariant | Camera finite invariant | All camera coordinates, angles, distance, and FOV are finite. | Exact | Current | O(1) |
| #bb.metric.cam.observer-cell-representability | Observer-cell representability | Whether #bb.metric.cam.observer-position maps into the supported i64 coordinate range. | Exact | Current | O(1) |
| #bb.metric.cam.bounds-center-offset | Bounds-center offset | Vector from observation anchor to the center of the occupied-world AABB. | Float | Current | O(1) |
| #bb.metric.cam.observer-bounds-relation | Observer-bounds relation | Inside/outside classification and signed distance from observer to the occupied-world AABB. | Float | Current | O(1) |

## Aiming and visual edit interaction

These metrics describe the center-ray interaction and GUI intent only. Canonical language edits,
tick batches, replay, and general mutation accounting are system concerns.

| Registry name | Metric | Precise definition | Exactness | Availability | Cost |
|---|---|---|---|---|---|
| #bb.metric.aim.ray-hit-record | Ray-hit record | For declared order k >= 1 along #bb.metric.cam.center-ray, the occupied coordinate, bedrock/writable kind, observer-to-entry distance, and entry point, ordered by entry distance with the declared lexicographic tie rule. Missing ranks are explicit. | Float/Exact | Current | O(N log N) for all ranks; O(N) for a fixed bounded rank |
| #bb.metric.aim.hit-normal | Hit normal | Outward voxel face normal at the rank-1 #bb.metric.aim.ray-hit-record entry point; edge/corner contacts may contain multiple axes. | Float | Instrument | O(N) |
| #bb.metric.aim.hit-face-coordinates | Hit face coordinates | Two local coordinates of the rank-1 #bb.metric.aim.ray-hit-record entry point on the contacted face. | Float | Instrument | O(N) |
| #bb.metric.aim.adjacent-coordinate | Adjacent coordinate | Coordinate immediately outside the rank-1 #bb.metric.aim.ray-hit-record coordinate across #bb.metric.aim.hit-normal, or none when unrepresentable. | Exact | Current | O(N) |
| #bb.metric.aim.adjacency-overflow | Adjacency overflow | #bb.metric.aim.adjacent-coordinate cannot be represented because the hit lies at an i64 coordinate boundary. | Exact | Instrument | O(N) |
| #bb.metric.aim.placement-target-source | Placement-target source | Hit-adjacent or anchor-cell fallback used to resolve a place intent. | Exact | Current | O(N) |
| #bb.metric.aim.placement-target | Placement target | Coordinate a right-click would attempt now, or none. | Exact | Current | O(N) |
| #bb.metric.aim.deletion-target | Deletion target | Coordinate a left-click would attempt now, or none. | Exact | Current | O(N) |
| #bb.metric.aim.target-current-state | Target current state | On/off state of the resolved GUI target before action. | Exact | Current | O(1) after target |
| #bb.metric.aim.target-protection | Target protection | Whether the resolved target is permanent bedrock. | Exact | Current | O(1) |
| #bb.metric.aim.target-contains-observer | Target contains observer | Whether turning the resolved target on would enclose #bb.metric.cam.observer-position. | Exact | Current | O(1) |
| #bb.metric.aim.action-eligibility | Action eligibility | Whether the current GUI intent resolves to an admitted exact edit. | Exact | Current | O(N) |
| #bb.metric.aim.action-block-reason | Action block reason | None, no hit, unrepresentable target, bedrock, observer enclosure, or future visual guard. | Exact | Instrument | O(N) |
| #bb.metric.aim.predicted-target-state | Predicted target state | On/off target state after the current GUI intent. | Exact | Current | O(1) after target |
| #bb.metric.aim.predicted-visual-state-change | Predicted visual state change | Whether the current GUI intent would mutate the target bit. | Exact | Current | O(1) after target |
| #bb.metric.aim.target-projected-footprint | Target projected footprint | Projected target bounding rectangle and clipped pixel area. | Float | Current | O(1) |
| #bb.metric.aim.crosshair-target-margin | Crosshair target margin | Signed screen-space distance from crosshair sample to target silhouette edge. | Float | Current | O(1) |
| #bb.metric.aim.ray-intersection-count | Ray intersection count | Number of occupied AABBs intersected anywhere on #bb.metric.cam.center-ray. | Float | Current | O(N) |
| #bb.metric.aim.occluded-depth-gap | Occluded depth gap | Entry-distance difference between rank-1 and rank-2 #bb.metric.aim.ray-hit-record values, or unavailable when rank 2 is absent. | Float | Current | O(N) |
| #bb.metric.aim.raycast-earliest-hit-tie-set | Raycast earliest-hit tie set | Count and coordinates tied at the raycast distance tolerance before lexicographic choice. | Float | Instrument | O(N) |
| #bb.metric.aim.nearest-miss-angle | Nearest miss angle | On a miss, minimum angular separation from #bb.metric.cam.center-ray to any occupied voxel silhouette. | Float | Current | O(N) |
| #bb.metric.aim.raycast-blocks-tested | Raycast blocks tested | Occupied AABBs examined by the most recent aim query. | Exact | Instrument | O(1) counter per tested bit |
| #bb.metric.aim.last-visual-intent | Last visual intent | Most recent GUI place/delete intent, distinct from exact-language edits. | Exact | Instrument | O(1) |
| #bb.metric.aim.last-visual-target | Last visual target | Resolved coordinate, source, and pre-action state for #bb.metric.aim.last-visual-intent. | Exact | Instrument | O(1) |
| #bb.metric.aim.last-visual-outcome | Last visual outcome | Changed, admitted no-op, rejected, or unresolved for #bb.metric.aim.last-visual-intent. | Exact | Instrument | O(1) |
| #bb.metric.aim.last-visual-rejection-reason | Last visual rejection reason | Exact guard or resolution failure behind a rejected/unresolved GUI action. | Exact | Instrument | O(1) |
| #bb.metric.aim.visual-observer-enclosure-prevention-count | Visual observer-enclosure prevention count | GUI place intents rejected because #bb.metric.aim.target-contains-observer was true. | Exact | Instrument | O(1) per action |
| #bb.metric.aim.visual-bedrock-protection-count | Visual bedrock-protection count | GUI delete intents neutralized by bedrock permanence. | Exact | Instrument | O(1) per action |
| #bb.metric.aim.visual-redundant-write-count | Visual redundant-write count | GUI intents admitted but requesting the target's existing writable state. | Exact | Instrument | O(1) per action |
| #bb.metric.aim.deferred-intent-retarget-event | Deferred intent retarget event | Capture-time target/source/prestate paired with consume-time target/source/prestate for each queued aim intent, classified unchanged, changed coordinate, became miss, became target, or changed eligibility. | Float/Exact | Instrument | O(N) at capture and consume |

## Viewport, zoom, and depth saturation

| Registry name | Metric | Precise definition | Exactness | Availability | Cost |
|---|---|---|---|---|---|
| #bb.metric.view.physical-viewport-size | Physical viewport size | Width and height of the simulation framebuffer in physical pixels. | Exact | Current | O(1) |
| #bb.metric.view.logical-window-size | Logical window size | OS logical width and height before display scaling. | Exact | Instrument | O(1) |
| #bb.metric.view.display-scale-factor | Display scale factor | Physical pixels per logical window unit. | Float | Instrument | O(1) |
| #bb.metric.view.viewport-pixel-count | Viewport pixel count | Product of #bb.metric.view.physical-viewport-size width and height. | Exact | Current | O(1) |
| #bb.metric.view.crosshair-sample-coordinate | Crosshair sample coordinate | Integer framebuffer coordinate selected as the exact aim sample. | Exact | Current | O(1) |
| #bb.metric.view.center-parity | Center parity | Odd/odd, odd/even, even/odd, or even/even centering case for #bb.metric.view.physical-viewport-size. | Exact | Current | O(1) |
| #bb.metric.view.active-zoom-interval | Active zoom interval | Minimum block-fill distance and dynamic one-pixel-fit maximum distance. | Float | Current | O(1) |
| #bb.metric.view.one-line-dolly-factor | One-line dolly factor | Multiplicative observation-distance change requested by exactly one wheel-line unit. | Float | Current | O(1) |
| #bb.metric.view.fit-anchor-penalty | Fit-anchor penalty | Difference between one-pixel-fit distance at the current anchor and after reanchoring to occupied-bounds center. | Float | Current | O(1), sixteen corner constraints |
| #bb.metric.view.top-saturation-state | Top-saturation state | Whether every occupied-bounds corner lies inside the central physical pixel. | Float | Current | O(1), eight corners |
| #bb.metric.view.top-saturation-residual | Top-saturation residual | Maximum projected corner excursion beyond the central half-pixel; nonpositive is saturated. | Float | Current | O(1), eight corners |
| #bb.metric.view.block-fill-endpoint-state | Block-fill endpoint state | Whether current observation distance is at the configured close-view endpoint. | Float | Current | O(1) |
| #bb.metric.view.projected-world-rectangle | Projected world rectangle | Min/max screen coordinates of projected occupied-bounds corners, including offscreen values. | Float | Current | O(1), eight corners |
| #bb.metric.view.clipped-bounds-coverage | Clipped bounds coverage | Area of #bb.metric.view.projected-world-rectangle clipped to the viewport, in pixels and as one stated fraction of P. | Float | Current | O(1) |
| #bb.metric.view.bounds-edge-margins | Bounds edge margins | Signed left, right, top, and bottom gaps from #bb.metric.view.projected-world-rectangle to viewport edges. | Float | Current | O(1) |
| #bb.metric.view.bounds-front-count | Bounds front count | Number of occupied-bounds corners with positive camera-space depth. | Exact | Current | O(1) |
| #bb.metric.view.bounds-fully-onscreen | Bounds fully onscreen | Every occupied-bounds corner is in front and inside the physical viewport. | Exact | Current | O(1) |
| #bb.metric.view.world-angular-extent | World angular extent | Minimum and maximum yaw/pitch covering the eight occupied-bounds corners. | Float | Current | O(1) |
| #bb.metric.view.active-scale-mode | Active scale mode | Inside-bit fill, whole-world aggregate, all-point, all-cube, or mixed point/cube rendering. | Exact | Current | O(N) worst case |
| #bb.metric.view.point-mode-bit-count | Point-mode bit count | Bits whose projected vertex rectangle is at most one pixel on both axes. | Float | Current | O(N) |
| #bb.metric.view.cube-mode-bit-count | Cube-mode bit count | Bits that proceed to face and triangle rasterization. | Float | Current | O(N) |
| #bb.metric.view.point-projection-collisions | Point-projection collisions | Point-mode bits grouped by their destination pixel before depth resolution. | Exact | Current | O(N) |
| #bb.metric.view.projected-bit-footprint-distribution | Projected bit-footprint distribution | Exact min, maximum, and fixed-bin histogram of projected bit rectangle area. | Float | Current | O(N) |
| #bb.metric.view.nearest-scale-transition-distance | Nearest scale-transition distance | Closest dolly distance at which any bit crosses the point/cube threshold. | Float | Current | O(N) |
| #bb.metric.view.bounds-depth-interval | Bounds depth interval | Nearest and farthest camera-space depths among occupied-bounds corners. | Float | Current | O(1) |
| #bb.metric.view.near-plane-margin | Near-plane margin | Minimum projected geometry depth minus the renderer near-plane value. | Float | Current | O(N) |
| #bb.metric.view.viewport-corner-coverage-mask | Viewport-corner coverage mask | Which of the four simulation-viewport corner pixels have finite world depth before overlays. | Exact | Current | O(1) |
| #bb.metric.view.near-far-pixel-scale-gradient | Near/far pixel-scale gradient | World-units-per-pixel at #bb.metric.view.bounds-depth-interval far depth divided by the scale at near depth. | Float | Current | O(1) |
| #bb.metric.view.resize-reframe-displacement | Resize-reframe displacement | Observer and anchor displacement caused specifically by a top-preserving viewport resize. | Float | Instrument | O(1) per resize |
| #bb.metric.view.zoom-clamp-cause | Zoom clamp cause | None, close-view endpoint, dynamic world-fit endpoint, or collision. | Exact | Instrument | O(1) per wheel action |
| #bb.metric.view.applied-wheel-transfer | Applied wheel transfer | Requested and realized change in log observation distance for one wheel action. | Float | Instrument | O(1) per wheel action |
| #bb.metric.view.occupied-frustum-relation-vector | Occupied frustum relation vector | Every occupied voxel AABB classified inside, intersecting, or outside against each active left, right, top, bottom, near, and declared far frustum plane. | Float | Instrument | O(N) |
| #bb.metric.view.top-saturation-reachability | Top saturation reachability | Reachable, unreachable, or closed reason plus a finite f64 camera witness at which all occupied bounds fit the designated central pixel while camera limits, representability, and collision clearance hold. | Float | Instrument | search/decision cost |
| #bb.metric.view.block-fill-reachability | Block-fill reachability | Reachable, unreachable, or closed reason plus an observer-clear camera witness at which one selected target's visible geometry covers every viewport sample under the declared renderer profile. | Float | Instrument | search plus render/reference checks |

## Renderer, rasterization, and visibility

Generic renderer duration, cadence, latency, and scheduling metrics are registered in system.md.
The counters here expose view-specific work and output.

| Registry name | Metric | Precise definition | Exactness | Availability | Cost |
|---|---|---|---|---|---|
| #bb.metric.render.render-branch | Render branch | Zero-size/empty, observer-inside-bit fill, one-pixel aggregate, or per-bit direct branch. | Exact | Current | O(1) |
| #bb.metric.render.render-dirty-state | Render dirty state | Whether camera/world/viewport/overlay state currently requires regeneration. | Exact | Current | O(1) |
| #bb.metric.render.idle-render-reuse | Idle render reuse | Whether the current tick intentionally reused the previous world framebuffer. | Exact | Current | O(1) |
| #bb.metric.render.pixels-cleared | Pixels cleared | Color/depth elements reset by the most recent render. | Exact | Instrument | O(1) counter |
| #bb.metric.render.bits-visited | Bits visited | Occupied coordinates traversed by the active render branch. | Exact | Instrument | O(1) counter per bit |
| #bb.metric.render.draw-bit-calls | Draw-bit calls | Bits entering per-bit projection. | Exact | Instrument | O(1) counter |
| #bb.metric.render.projection-calls | Projection calls | Camera project operations issued for vertices, centers, bounds, and aggregate point. | Exact | Instrument | O(1) counter |
| #bb.metric.render.valid-projected-vertices | Valid projected vertices | Cube vertices producing finite positive-depth projections. | Exact | Instrument | O(1) per vertex |
| #bb.metric.render.behind-plane-vertices | Behind-plane vertices | Cube vertices rejected on or behind the camera plane. | Exact | Instrument | O(1) per vertex |
| #bb.metric.render.unprojectable-bits | Unprojectable bits | Draw-bit calls for which all eight vertex projections fail. | Exact | Instrument | O(1) per bit |
| #bb.metric.render.wholly-offscreen-bits | Wholly offscreen bits | Projected bit rectangles with no overlap with the physical viewport. | Float | Instrument | O(1) per bit |
| #bb.metric.render.partially-clipped-bits | Partially clipped bits | Projected bit rectangles crossing at least one viewport edge. | Float | Instrument | O(1) per bit |
| #bb.metric.render.faces-tested | Faces tested | Cube faces considered before culling and projection checks. | Exact | Instrument | O(1) counter |
| #bb.metric.render.backfaces-culled | Backfaces culled | Faces rejected by the normal-to-camera sign test. | Exact | Instrument | O(1) counter |
| #bb.metric.render.face-projection-rejects | Face projection rejects | Front-facing faces missing one or more projected corners. | Exact | Instrument | O(1) counter |
| #bb.metric.render.faces-submitted | Faces submitted | Faces converted to two triangles. | Exact | Instrument | O(1) counter |
| #bb.metric.render.triangles-submitted | Triangles submitted | Calls made to triangle rasterization. | Exact | Instrument | O(1) counter |
| #bb.metric.render.near-plane-triangle-rejects | Near-plane triangle rejects | Submitted triangles discarded because any vertex depth is at/below the near plane. | Exact | Instrument | O(1) counter |
| #bb.metric.render.degenerate-triangle-rejects | Degenerate triangle rejects | Submitted triangles rejected for nonfinite or near-zero projected area. | Exact | Instrument | O(1) counter |
| #bb.metric.render.triangles-rasterized | Triangles rasterized | Triangles entering clipped bounding-box pixel loops. | Exact | Instrument | O(1) counter |
| #bb.metric.render.candidate-raster-samples | Candidate raster samples | Sum of clipped triangle bounding-box pixel counts, Q. | Exact | Instrument | O(1) per triangle |
| #bb.metric.render.covered-triangle-samples | Covered triangle samples | Candidate samples passing the barycentric inside test. | Exact | Instrument | O(1) per sample |
| #bb.metric.render.invalid-inverse-depth-samples | Invalid inverse-depth samples | Covered samples rejected for nonpositive or nonfinite inverse depth. | Exact | Instrument | O(1) per sample |
| #bb.metric.render.plot-attempts | Plot attempts | Point and triangle fragments submitted to plot. | Exact | Instrument | O(1) per fragment |
| #bb.metric.render.offscreen-point-plots | Offscreen point plots | Point-mode or aggregate plot attempts rejected outside framebuffer bounds. | Exact | Instrument | O(1) per point |
| #bb.metric.render.depth-passes | Depth passes | In-bounds fragments replacing the stored winner under the declared total rule: nearer finite depth wins; exact depth ties select the lower RGB word. | Exact | Instrument | O(1) per fragment |
| #bb.metric.render.depth-rejects | Depth rejects | In-bounds fragments retained as losers under the declared depth-plus-RGB total winner rule. | Exact | Instrument | O(1) per fragment |
| #bb.metric.render.fragment-contention | Fragment contention | Exact histogram of in-bounds fragment-test counts per pixel. | Exact | Instrument | O(P) memory and O(Q) updates |
| #bb.metric.render.world-pixel-count | World-pixel count | Pixels with finite world depth before crosshair composition. | Exact | Current | O(P) |
| #bb.metric.render.world-pixel-rectangle | World-pixel rectangle | Tight physical-pixel rectangle containing all finite world depth. | Exact | Current | O(P) |
| #bb.metric.render.visible-depth-distribution | Visible depth distribution | Minimum, maximum, and fixed-bin histogram over finite depth-buffer values. | Float | Current | O(P) |
| #bb.metric.render.depth-discontinuity-edges | Depth-discontinuity edges | Neighbor pairs whose finite depths differ beyond one declared scale-relative threshold. | Float | Current | O(P) |
| #bb.metric.render.visible-bit-count | Visible bit count | Distinct occupied coordinates owning at least one final world pixel. | Exact | Instrument | O(P), owner buffer |
| #bb.metric.render.fully-occluded-bit-count | Fully occluded bit count | Projected, non-offscreen bits owning no final world pixel. | Exact | Instrument | O(N + P), owner buffer |
| #bb.metric.render.visible-pixel-ownership-distribution | Visible-pixel ownership distribution | Exact pixel count for every visible coordinate. | Exact | Instrument | O(P), owner buffer |
| #bb.metric.render.nearest-visible-owner | Nearest visible owner | Coordinate owning the minimum finite depth. | Float | Instrument | O(P), owner buffer |
| #bb.metric.render.farthest-visible-owner | Farthest visible owner | Coordinate owning the maximum finite depth. | Float | Instrument | O(P), owner buffer |
| #bb.metric.render.center-pixel-record | Center-pixel record | Final color, pre-overlay world depth, and owning coordinate at #bb.metric.view.crosshair-sample-coordinate. | Float | Instrument | O(1), owner buffer |
| #bb.metric.render.final-palette-histogram | Final palette histogram | Exact count of every RGB value after crosshair composition. | Exact | Current | O(P) |
| #bb.metric.render.bedrock-visible-pixels | Bedrock visible pixels | Pixels owned by bedrock, independent of orange-color collisions. | Exact | Instrument | O(P), owner buffer |
| #bb.metric.render.aggregate-pixel-record | Aggregate-pixel record | Pixel coordinate, depth, and color representing the complete world in aggregate mode. | Float | Current | O(1) |
| #bb.metric.render.owner-boundary-edges | Owner-boundary edges | Neighboring finite-depth pixel pairs owned by different occupied coordinates. | Exact | Instrument | O(P), owner buffer |
| #bb.metric.render.framebuffer-color-entropy | Framebuffer color entropy | Shannon entropy of the final RGB histogram. | Float | Current | O(P) |
| #bb.metric.render.foreground-edge-count | Foreground edge count | Horizontal and vertical finite-depth/background transitions. | Exact | Current | O(P) |
| #bb.metric.render.foreground-components | Foreground components | Count and areas of connected finite-depth pixel components under one declared adjacency. | Exact | Current | O(P) |
| #bb.metric.render.color-buffer-allocation | Color-buffer allocation | Length, capacity, and bytes of the RGB vector. | Exact | Current | O(1) |
| #bb.metric.render.depth-buffer-allocation | Depth-buffer allocation | Length, capacity, and bytes of the depth vector. | Exact | Current | O(1) |
| #bb.metric.render.owner-buffer-allocation | Owner-buffer allocation | Length, capacity, and bytes of optional coordinate ownership storage. | Exact | Instrument | O(1) |
| #bb.metric.render.buffer-reallocations | Buffer reallocations | Resize operations that change any renderer-buffer capacity, with bytes acquired. | Exact | Instrument | O(1) per resize |
| #bb.metric.render.crosshair-samples-in-bounds | Crosshair samples in bounds | Intended arm samples landing inside the simulation viewport; nominally twelve. | Exact | Current | O(1) |
| #bb.metric.render.crosshair-samples-clipped | Crosshair samples clipped | Intended arm samples outside a small viewport. | Exact | Current | O(1) |
| #bb.metric.render.crosshair-world-inversions | Crosshair world inversions | Arm samples whose pre-overlay depth is finite. | Exact | Current | O(1) |
| #bb.metric.render.crosshair-background-inversions | Crosshair background inversions | Arm samples whose pre-overlay depth is infinite. | Exact | Current | O(1) |
| #bb.metric.render.final-framebuffer-storage-contract | Final framebuffer storage contract | Protocol tuple for the post-overlay CPU buffer: 32-bit word interpretation, `0x00RRGGBB` channel mapping, unused-high-byte rule, row-major origin/order, and stride rule. This is format identity, not dimensions or allocation. | Exact | Current/declared | O(1) |
| #bb.metric.render.backend-submission-event | Backend submission event | Referenced frame tag, surface generation/backend policy and format, mapped-buffer acquisition result, mapped-copy verdict, and exact submission-API return for each attempt. The current Windows admission checks the requested and returned `SetDIBitsToDevice` scanline counts plus Win32 error and `ReleaseDC` outcome, and publishes frame receipts only for the complete-row case. Success means checked GDI acceptance, not compositor or monitor output. | Exact observed event | Current Windows admission check; durable event retention and other backends Instrument | O(1) beyond copy check |
| #bb.metric.render.fragment-contest-outcome-vector | Fragment contest outcome vector | Every in-bounds plot classified by prior-depth class, `total_cmp` relation, source relation, replacement/retention, and contender/incumbent colors. Exact-tie totals are views. | Float/Exact | Instrument | O(1) per fragment, retained witnesses optional |
| #bb.metric.render.barycentric-tolerance-admission-vector | Barycentric tolerance admission vector | Strict-inside, epsilon-only admitted, and rejected raster samples plus the most-negative admitted barycentric weight under the declared tolerance. | Float/Exact | Instrument | O(1) per candidate sample |
| #bb.metric.render.framebuffer-colorimetry-contract | Framebuffer colorimetry contract | Tagged declaration of transfer function, primaries, white point, channel range, alpha/unused-byte meaning, and intended output color space; storage layout alone does not define optical meaning. | Exact declaration | Instrument | O(1) |
| #bb.metric.render.render-dirty-cause-set | Render dirty cause set | Exact causes accumulated since the previous render—initialization, look, translation, fit, dolly, edit, resize, or overlay—paired with whether each changed canonical render input. | Exact | Instrument | O(1) per cause |
| #bb.metric.render.produced-frame-disposition-relation | Produced frame disposition relation | Every produced frame tag/source tick/ordinal mapped to submitted, intentionally headless/unsubmitted, superseded before submission, reused, or failed, with successor or submission tag when present. A headless CPU-frame commitment is production evidence, not a backend receipt. | Exact | Now for explicit headless disposition; Instrument remaining live relations | O(1) per frame |

## Raw input

System timing owns event latency and event-rate windows. This section records the raw/current input
primitives and exact event counters needed to interpret observer behavior.

| Registry name | Metric | Precise definition | Exactness | Availability | Cost |
|---|---|---|---|---|---|
| #bb.metric.input.mouse-capture-state | Mouse capture state | Captured or free observer-look state. | Exact | Current | O(1) |
| #bb.metric.input.window-focus-state | Window focus state | Focused or unfocused application window. | Exact | Instrument | O(1) |
| #bb.metric.input.cursor-grab-mode | Cursor grab mode | Locked, confined fallback, failed, or none. | Exact | Instrument | O(1) |
| #bb.metric.input.cursor-visibility | Cursor visibility | Last visibility state requested from the window. | Exact | Instrument | O(1) |
| #bb.metric.input.movement-key-mask | Movement key mask | Held state of forward, backward, left, right, up, and down controls. | Exact | Current | O(1) |
| #bb.metric.input.active-movement-key-count | Active movement-key count | Number of true bits in #bb.metric.input.movement-key-mask. | Exact | Current | O(1) |
| #bb.metric.input.opposing-axis-mask | Opposing-axis mask | Local axes on which positive and negative controls are simultaneously held. | Exact | Current | O(1) |
| #bb.metric.input.raw-local-movement-vector | Raw local movement vector | Pre-normalization axis result in {-1, 0, 1} cubed. | Exact | Current | O(1) |
| #bb.metric.input.consumed-movement-direction | Consumed movement direction | Unit local direction derived from #bb.metric.input.raw-local-movement-vector, or none. | Float | Current | O(1) |
| #bb.metric.input.zoom-scaled-movement-speed | Zoom-scaled movement speed | Current world units per second selected from observation distance and minimum speed. | Float | Current | O(1) |
| #bb.metric.input.requested-movement-step | Requested movement step | #bb.metric.input.consumed-movement-direction multiplied by #bb.metric.input.zoom-scaled-movement-speed and fixed tick duration. | Float | Current | O(1) |
| #bb.metric.input.pending-mouse-delta | Pending mouse delta | Accumulated raw device-motion x and y awaiting tick consumption. | Float | Current | O(1) |
| #bb.metric.input.pending-mouse-event-count | Pending mouse-event count | Raw motion events merged into #bb.metric.input.pending-mouse-delta. | Exact | Instrument | O(1) per event |
| #bb.metric.input.mouse-sensitivity | Mouse sensitivity | Radians applied per raw device-motion unit. | Float | Current | O(1) |
| #bb.metric.input.pending-wheel-delta | Pending wheel delta | Clamped wheel-line units awaiting tick consumption. | Float | Current | O(1) |
| #bb.metric.input.wheel-source-counters | Wheel source counters | Exact line-delta and pixel-delta event counts. | Exact | Instrument | O(1) per event |
| #bb.metric.input.wheel-clamp-count | Wheel clamp count | Wheel additions truncated by the pending [-64, 64] clamp. | Exact | Instrument | O(1) per event |
| #bb.metric.input.pixel-wheel-conversion-residual | Pixel-wheel conversion residual | Pixel delta divided by 120 minus the wheel-line delta actually accumulated. | Float | Instrument | O(1) per event |
| #bb.metric.input.mouse-button-counters | Mouse-button counters | Left, right, and other press-event counts. | Exact | Instrument | O(1) per event |
| #bb.metric.input.capture-only-click-count | Capture-only click count | Clicks consumed to recapture the mouse rather than issue a visual edit intent. | Exact | Instrument | O(1) per event |
| #bb.metric.input.fit-request-pending | Fit request pending | Whether a fit-world key press awaits tick consumption. | Exact | Current | O(1) |
| #bb.metric.input.fit-request-count | Fit request count | Accepted fit-world press events. | Exact | Instrument | O(1) per event |
| #bb.metric.input.raw-event-counters | Raw event counters | Keyboard, device motion, wheel, button, focus, resize, and redraw counts. | Exact | Instrument | O(1) per event |
| #bb.metric.input.input-bearing-tick | Input-bearing tick | Whether the current tick consumes movement, look, wheel, fit, or GUI edit input. | Exact | Instrument | O(1) |
| #bb.metric.input.keyboard-repeat-count | Keyboard repeat count | Press events received while the corresponding key is already held. | Exact | Instrument | O(1) per event |
| #bb.metric.input.ignored-synthetic-key-count | Ignored synthetic-key count | Synthetic keyboard events excluded by the event matcher. | Exact | Instrument | O(1) per event |
| #bb.metric.input.unsupported-input-counters | Unsupported-input counters | Unbound keyboard and mouse-button events. | Exact | Instrument | O(1) per event |
| #bb.metric.input.capture-transition-counters | Capture transition counters | Successful capture acquisitions and releases. | Exact | Instrument | O(1) per transition |
| #bb.metric.input.capture-failure-count | Capture failure count | Attempts where locked and confined grab both fail. | Exact | Instrument | O(1) per attempt |
| #bb.metric.input.focus-loss-count | Focus-loss count | Focus losses that release capture and clear motion state. | Exact | Instrument | O(1) per event |
| #bb.metric.input.movement-chord-histogram | Movement chord histogram | Exact counts of the 64 possible held movement-key masks when ticks consume input. | Exact | Instrument | O(1) per input-bearing tick |
| #bb.metric.input.movement-direction-reversals | Movement-direction reversals | Sign changes in nonzero movement intent, recorded per local axis. | Exact | Instrument | O(1) per tick |
| #bb.metric.input.mouse-direction-reversals | Mouse-direction reversals | Sign changes in nonzero consumed mouse delta, recorded per axis. | Exact | Instrument | O(1) per tick |
| #bb.metric.input.wheel-direction-reversals | Wheel-direction reversals | Changes between inward and outward consumed wheel intent. | Exact | Instrument | O(1) per wheel action |
| #bb.metric.input.raw-input-device-count | Raw input device count | Number of distinct DeviceId values producing input in the session. | Exact | Instrument | O(1) average per event, O(devices) memory |
| #bb.metric.input.applied-mouse-transfer | Applied mouse transfer | Consumed raw mouse delta, requested yaw/pitch change, realized wrapped/clamped angle change, and pitch-clamp loss for one fixed tick. | Float | Instrument | O(1) per consuming tick |
| #bb.metric.input.focus-release-disposition-event | Focus/release disposition event | Trigger plus before/after held keys, mouse delta, wheel delta, fit request, and queued edits, each classified cleared or retained. | Exact/Float | Instrument | O(1) per boundary |
| #bb.metric.input.pending-analog-finiteness | Pending analog finiteness | Finite/nonfinite verdict over pending mouse X/Y and wheel values, retaining offending IEEE-754 bits and source event class. | Exact over bits | Instrument | O(1) per accumulation and consumption |
| #bb.metric.input.effective-key-transition-event | Effective key transition event | Event ordinal, physical key, press/release, prior/resulting held bit, repeat/effective/unsupported class, and observed tick for every keyboard callback. | Exact | Instrument | O(1) per key event |
| #bb.metric.input.raw-input-occurrence-relation | Raw-input occurrence relation | Total ordered ordinal, event class, canonical raw payload bits, device/window identity, capture/focus state, receiving tick, and optional host timestamp for every input callback before coalescing. | Exact/Host timestamp | Instrument | O(1) per event plus retained log |
| #bb.metric.input.wheel-clamp-loss-event | Wheel-clamp loss event | Prior pending value, incoming line-equivalent delta, unclamped sum, accepted clamped sum, and signed discarded delta for every wheel addition. | Float | Instrument | O(1) per wheel event |

## Observer collision

| Registry name | Metric | Precise definition | Exactness | Availability | Cost |
|---|---|---|---|---|---|
| #bb.metric.collision.observer-clear-invariant | Observer-clear invariant | A zero-length point sweep at #bb.metric.cam.observer-position intersects no occupied closed AABB. | Exact | Current | O(N) |
| #bb.metric.collision.penetration-record | Penetration record | Maximum signed penetration depth and owning coordinate; valid state has no positive penetration. | Float | Current | O(N) |
| #bb.metric.collision.nearest-occupied-record | Nearest occupied record | Coordinate, closest AABB point, and Euclidean surface gap nearest #bb.metric.cam.observer-position. | Float | Current | O(N) |
| #bb.metric.collision.bedrock-clearance | Bedrock clearance | Euclidean surface gap from #bb.metric.cam.observer-position to the origin voxel AABB. | Float | Current | O(1) |
| #bb.metric.collision.requested-sweep | Requested sweep | Start point, requested delta, and stable segment length for the latest physical camera move. | Float | Instrument | O(1) per move |
| #bb.metric.collision.accepted-sweep | Accepted sweep | Allowed delta and endpoint after collision clearance. | Float | Instrument | O(1) per move |
| #bb.metric.collision.prevented-displacement | Prevented displacement | Vector and length difference between #bb.metric.collision.requested-sweep and #bb.metric.collision.accepted-sweep. | Float | Instrument | O(1) per move |
| #bb.metric.collision.collision-result | Collision result | Clear, shortened, or fully blocked classification for the latest requested move. | Exact | Instrument | O(1) per move |
| #bb.metric.collision.first-collision-coordinate | First collision coordinate | Lexicographically deterministic earliest occupied coordinate on the requested segment. | Exact | Instrument | O(1) retained from sweep |
| #bb.metric.collision.first-collision-distance | First collision distance | Segment-start to first closed-AABB entry. | Float | Instrument | O(1) retained from sweep |
| #bb.metric.collision.contact-point | Contact point | Segment start plus normalized requested delta times #bb.metric.collision.first-collision-distance. | Float | Instrument | O(1) |
| #bb.metric.collision.contact-normal-set | Contact normal set | One, two, or three outward axes active at the first contact. | Float | Instrument | O(N) |
| #bb.metric.collision.contact-topology | Contact topology | Face, edge, corner, start touching, or start inside. | Exact | Instrument | O(1) after normal set |
| #bb.metric.collision.applied-clearance | Applied clearance | Exact pre-contact distance retained by collision_limited_delta. | Float | Instrument | O(1) |
| #bb.metric.collision.numeric-clearance-component | Numeric clearance component | Coordinate-scale times f64 epsilon times sixteen used by the limiter. | Float | Instrument | O(1) |
| #bb.metric.collision.post-move-nearest-gap | Post-move nearest gap | Minimum occupied-AABB surface gap after #bb.metric.collision.accepted-sweep. | Float | Current | O(N) |
| #bb.metric.collision.sweep-kind | Sweep kind | Local translation, inward dolly, or outward dolly. | Exact | Instrument | O(1) |
| #bb.metric.collision.impact-approach-cosine | Impact approach cosine | Dot product of normalized requested direction and inward contact normal. | Float | Instrument | O(1) |
| #bb.metric.collision.zero-distance-hit | Zero-distance hit | First contact occurs at segment distance zero. | Exact | Instrument | O(1) |
| #bb.metric.collision.swept-intersection-count | Swept intersection count | Occupied AABBs touched anywhere on #bb.metric.collision.requested-sweep. | Exact | Instrument | O(N) |
| #bb.metric.collision.sweep-earliest-hit-tie-set | Sweep earliest-hit tie set | Coordinates sharing the minimum entry fraction before lexicographic selection. | Float | Instrument | O(N) |
| #bb.metric.collision.sweep-blocks-tested | Sweep blocks tested | Occupied AABBs examined by the sweep; currently normally N. | Exact | Instrument | O(1) counter per bit |
| #bb.metric.collision.suppressed-tangential-displacement | Suppressed tangential displacement | Tangential component discarded because response stops the full vector rather than sliding. | Float | Instrument | O(1) |
| #bb.metric.collision.sweep-axis-mask | Sweep axis mask | World axes with nonzero displacement in the requested segment. | Exact | Instrument | O(1) |
| #bb.metric.collision.closed-tangent-contact-count | Closed-tangent contact count | Contacts caused solely by closed face, edge, or corner tangency. | Exact | Instrument | O(1) per contact |
| #bb.metric.collision.contact-topology-counters | Contact-topology counters | Exact face, edge, corner, start-touching, and start-inside event counts. | Exact | Instrument | O(1) per contact |
| #bb.metric.collision.unique-contacted-coordinates | Unique contacted coordinates | Cardinality of occupied coordinates ever selected as first collision. | Exact | Instrument | O(1) average per contact, O(U) memory |
| #bb.metric.collision.repeated-coordinate-streak | Repeated-coordinate streak | Consecutive collision-bearing movement requests selecting the same coordinate. | Exact | Instrument | O(1) |
| #bb.metric.collision.maximum-safe-forward-step | Maximum safe forward step | First-hit distance minus required clearance along the current requested direction. | Float | Current | O(N) |
| #bb.metric.collision.traversed-empty-cells | Traversed empty cells | Distinct coordinate cells crossed by the accepted point segment. | Exact | Instrument | O(crossed cells) |
| #bb.metric.collision.swept-cell-aabb | Swept cell AABB | Inclusive integer-cell bounds and volume enclosing the requested segment. | Float | Current | O(1) |
| #bb.metric.collision.fit-world-clear-postcondition | Fit-world clear postcondition | #bb.metric.collision.observer-clear-invariant immediately after a fit-world reframe. | Exact | Current | O(N) |
| #bb.metric.collision.resize-reframe-clear-postcondition | Resize-reframe clear postcondition | #bb.metric.collision.observer-clear-invariant immediately after a top-preserving resize reframe. | Exact | Instrument | O(N) |
| #bb.metric.collision.collision-invariant-breach-count | Collision invariant breach count | Completed camera moves ending inside or on an occupied closed AABB. | Exact | Instrument | O(N) per checked move |
| #bb.metric.collision.independent-sweep-reference-mismatch-vector | Independent sweep reference mismatch vector | Production versus independent exact-dyadic or rational slab result: hit/miss, first coordinate, global entry fraction, and f64 ULP residual over a declared immutable corpus. | Exact reference/Float residual | Instrument | O(corpus*N) |
| #bb.metric.collision.sweep-segmentation-consistency | Sweep segmentation consistency | Earliest geometric hit coordinate and global path fraction for one finite segment evaluated whole versus declared partitions; response clearance is excluded. | Float/Exact | Instrument | paired sweep cost |
| #bb.metric.collision.post-edit-observer-clear-postcondition | Post-edit observer-clear postcondition | Pass/fail and offending coordinate immediately after every admitted off-to-on transition, partitioned by edit source. | Exact | Instrument | O(N) per admitted mint |

## Sidebar self-metrics

Sidebar scheduling age and refresh latency use system.md. These metrics make the panel's geometry,
content loss, and interference visible without allowing the panel to affect simulation state.

| Registry name | Metric | Precise definition | Exactness | Availability | Cost |
|---|---|---|---|---|---|
| #bb.metric.sidebar.sidebar-visibility | Sidebar visibility | Whether the diagnostic panel is enabled. | Exact | Current snapshot | O(1) |
| #bb.metric.sidebar.sidebar-rectangle | Sidebar rectangle | Panel origin, width, and height in physical pixels. | Exact | Current snapshot | O(1) |
| #bb.metric.sidebar.simulation-viewport-rectangle | Simulation viewport rectangle | World-render region remaining after #bb.metric.sidebar.sidebar-rectangle is reserved. | Exact | Current snapshot | O(1) |
| #bb.metric.sidebar.row-capacity | Row capacity | Complete text rows fitting at current panel dimensions and glyph scale. | Exact | Current raster | O(1) |
| #bb.metric.sidebar.displayed-metric-count | Displayed metric count | Metric rows actually emitted in the current panel layout. | Exact | Current raster, displayed as predecessor | O(1) |
| #bb.metric.sidebar.hidden-metric-count | Hidden metric count | Registered rows hidden by section collapse, filter, or physical overflow. | Exact | Instrument | O(rows) |
| #bb.metric.sidebar.visible-section-count | Visible section count | Expanded metric categories in the panel. | Exact | Current policy/snapshot | O(sections) |
| #bb.metric.sidebar.scroll-position | Scroll position | First visible logical row or category offset. | Exact | Instrument | O(1) |
| #bb.metric.sidebar.glyph-count | Glyph count | Characters emitted by the current panel snapshot. | Exact | Current raster, displayed as predecessor | O(text) |
| #bb.metric.sidebar.lit-text-pixels | Lit text pixels | Foreground bitmap-font pixels drawn into #bb.metric.sidebar.sidebar-rectangle. | Exact | Current raster, displayed as predecessor | O(sidebar pixels) |
| #bb.metric.sidebar.changed-field-count | Changed field count | Displayed metric values differing from the prior panel snapshot. | Exact | Instrument | O(rows) |
| #bb.metric.sidebar.unavailable-field-count | Unavailable field count | Rows showing unavailable, overflow, unsupported, or instrumentation-disabled. | Exact | Current raster, displayed as predecessor | O(rows) |
| #bb.metric.sidebar.numeric-truncation-count | Numeric truncation count | Values whose full chosen representation does not fit the assigned value column. | Exact | Instrument | O(rows) |
| #bb.metric.sidebar.clipped-text-count | Clipped text count | Labels or values physically clipped by #bb.metric.sidebar.sidebar-rectangle. | Exact | Current raster, displayed as predecessor | O(rows) |
| #bb.metric.sidebar.wrapped-line-count | Wrapped line count | Logical metric rows consuming more than one physical text row. | Exact | Instrument | O(rows) |
| #bb.metric.sidebar.formatting-error-bound | Formatting error bound | Maximum numeric error introduced by the chosen displayed decimal precision. | Float | Instrument | O(rows) |
| #bb.metric.sidebar.coordinate-precision-warnings | Coordinate precision warnings | Coordinate rows whose displayed precision hides a block-scale or f64-alias distinction. | Exact | Instrument | O(rows) |
| #bb.metric.sidebar.active-alert-count | Active alert count | Currently displayed warning/error invariant rows. | Exact | Instrument | O(rows) |
| #bb.metric.sidebar.highest-alert-severity | Highest alert severity | Maximum severity under the panel's fixed categorical scheme. | Exact | Instrument | O(rows) |
| #bb.metric.sidebar.crosshair-layout-residual | Crosshair layout residual | Pixel distance from #bb.metric.view.crosshair-sample-coordinate to the center of #bb.metric.sidebar.simulation-viewport-rectangle, never the full window. | Float | Instrument | O(1) |
| #bb.metric.sidebar.sidebar-input-leaks | Sidebar input leaks | Pointer/wheel events inside #bb.metric.sidebar.sidebar-rectangle that alter world or observer state. | Exact | Instrument | O(1) per event |
| #bb.metric.sidebar.minimum-text-contrast | Minimum text contrast | Lowest contrast ratio among foreground/background/status palette pairs currently used. | Float | Instrument | O(palette) |
| #bb.metric.sidebar.glyph-scale | Glyph scale | Physical width, height, and spacing of one bitmap-font cell. | Exact | Current policy | O(1) |
| #bb.metric.sidebar.sidebar-buffer-allocation | Sidebar buffer allocation | Bytes and capacity attributable only to sidebar composition. | Exact | Instrument | O(1) |
| #bb.metric.sidebar.dirty-rows | Dirty rows | Panel rows requiring repaint because text or style changed. | Exact | Instrument | O(rows) |
| #bb.metric.sidebar.repaint-pixels | Repaint pixels | Physical sidebar pixels rewritten for #bb.metric.sidebar.dirty-rows. | Exact | Instrument | O(sidebar pixels) |
| #bb.metric.sidebar.exactness-class-counts | Exactness-class counts | Displayed rows partitioned into Exact, Float, Sampled, and unavailable. | Exact | Instrument | O(rows) |
| #bb.metric.sidebar.minimum-simulation-view-breach | Minimum simulation-view breach | Whether #bb.metric.sidebar.simulation-viewport-rectangle is below the renderer's supported minimum dimensions. | Exact | Current layout | O(1) |
| #bb.metric.sidebar.layout-reflow-count | Layout reflow count | Panel row/column arrangements changed by resize, visibility, or representation growth. | Exact | Instrument | O(1) per reflow |

## Visual correctness diagnostics

Sampled reference-ray diagnostics explicitly report R and never promote sample agreement to a
universal rendering proof. Formal proof status remains outside this file.

| Registry name | Metric | Precise definition | Exactness | Availability | Cost |
|---|---|---|---|---|---|
| #bb.metric.visual.render-world-purity-residual | Render world purity residual | Exact field-by-field world-state difference before versus after rendering. | Exact | Instrument | O(N), snapshots |
| #bb.metric.visual.render-camera-purity-residual | Render camera purity residual | Exact camera-field difference before versus after rendering. | Float | Instrument | O(1), snapshots |
| #bb.metric.visual.repeated-render-mismatch | Repeated-render mismatch | Pixel/depth entries differing across two renders of identical world, camera, and viewport state. | Exact | Instrument | O(render + P) |
| #bb.metric.visual.iteration-order-mismatch | Iteration-order mismatch | Pixel/depth entries changed by a deliberately permuted occupied-bit traversal. | Exact | Instrument | O(render + P) |
| #bb.metric.visual.depth-coverage-incoherence | Depth-coverage incoherence | Pixels whose pre-overlay world-color/background state disagrees with finite/infinite depth. | Exact | Instrument | O(P) |
| #bb.metric.visual.owner-depth-color-incoherence | Owner-depth-color incoherence | Owner entries lacking finite depth or carrying a color illegal for that owner/face. | Exact | Instrument | O(P), owner buffer |
| #bb.metric.visual.center-ray-owner-agreement | Center ray-owner agreement | Whether the rank-1 #bb.metric.aim.ray-hit-record coordinate equals the coordinate owning #bb.metric.render.center-pixel-record. | Exact | Instrument | O(N), owner buffer |
| #bb.metric.visual.center-depth-residual | Center depth residual | Absolute difference between the rank-1 #bb.metric.aim.ray-hit-record entry distance and the center pixel's world depth. | Float | Instrument | O(N) |
| #bb.metric.visual.projection-ray-inverse-residual | Projection-ray inverse residual | Angular error after projecting a probe point and regenerating a ray through that physical sample. | Float | Current | O(1) per probe |
| #bb.metric.visual.anchor-center-residual | Anchor-center residual | Pixel distance between #bb.metric.cam.observation-anchor projection and the exact simulation-view center. | Float | Current | O(1) |
| #bb.metric.visual.crosshair-center-preservation | Crosshair-center preservation | Whether crosshair composition leaves #bb.metric.view.crosshair-sample-coordinate color unchanged. | Exact | Instrument | O(1), pre-overlay color |
| #bb.metric.visual.crosshair-arm-symmetry-residual | Crosshair arm symmetry residual | Position/color mismatch between opposing arms after accounting for underlying pre-overlay colors. | Exact | Instrument | O(1) |
| #bb.metric.visual.crosshair-arm-geometry-residual | Crosshair arm-geometry residual | Arm sample offsets that disagree with the configured gap, length, axis alignment, or four-way symmetry. | Exact | Current | O(1) |
| #bb.metric.visual.aggregate-branch-equivalence | Aggregate-branch equivalence | Aggregate branch active exactly when bounds fit one pixel, excluding zero-size/inside-bit cases. | Float | Current | O(1) |
| #bb.metric.visual.aggregate-pixel-count-residual | Aggregate pixel-count residual | Finite-depth world-pixel count minus one while aggregate branch is active. | Exact | Current | O(P) |
| #bb.metric.visual.aggregate-placement-residual | Aggregate placement residual | Pixel distance from aggregate output to the designated central physical pixel. | Float | Current | O(1) |
| #bb.metric.visual.bedrock-palette-violations | Bedrock palette violations | Bedrock owners using writable palette or writable owners using bedrock-only palette. | Exact | Instrument | O(P), owner buffer |
| #bb.metric.visual.block-fill-coverage-residual | Block-fill coverage residual | Background pixel count when the close-view geometry is expected to fill the frame. | Exact | Current | O(P) |
| #bb.metric.visual.observer-inside-render-entry-count | Observer-inside render entry count | Entries into the emergency camera-inside-bit full-frame branch. | Exact | Instrument | O(1) per render |
| #bb.metric.visual.depth-acceptance-violations | Depth acceptance violations | Accepted fragments not preferred to the previous stored winner under the declared nearer-depth then lower-RGB total rule. | Float/Exact | Instrument | O(1) per fragment |
| #bb.metric.visual.near-tie-order-sensitivity | Near-tie order sensitivity | Pixels whose owner/color changes when near-equal fragments arrive in another order. | Float | Instrument | O(Q) |
| #bb.metric.visual.near-plane-straddle-losses | Near-plane straddle losses | Triangles discarded wholesale despite geometrically crossing into visible space. | Float | Instrument | O(1) per triangle |
| #bb.metric.visual.invalid-projection-count | Invalid projection count | Nonfinite projected x, y, or depth from otherwise finite camera and world inputs. | Exact | Instrument | O(1) per projection |
| #bb.metric.visual.coordinate-center-alias-groups | Coordinate-center alias groups | Sets of distinct occupied integer coordinates mapping to identical f64 cube centers. | Exact | Current | O(N) average with set |
| #bb.metric.visual.projected-center-alias-groups | Projected-center alias groups | Distinct geometric centers producing identical raw projected x and y before pixel flooring. | Float | Current | O(N) average with set |
| #bb.metric.visual.invalid-pixel-conversion-count | Invalid pixel-conversion count | Projected values reaching integer conversion without first satisfying explicit finite/range checks. | Exact | Instrument | O(1) per conversion |
| #bb.metric.visual.out-of-bounds-write-attempts | Out-of-bounds write attempts | Attempted renderer memory writes outside color/depth/owner buffers. | Exact | Instrument | O(1) per attempted write |
| #bb.metric.visual.sampled-raster-false-negatives | Sampled raster false negatives | Among R declared reference pixels, independently computed analytic face hits whose candidate raster depth is infinite, retaining pixel indices. The current Python six-face oracle checks the complete pixel complement of two named direct-cube fixtures; this finite corpus is not a universal render proof. | Sampled, exhaustive within named frames | Current bounded independent oracle | O(RN), reference rays |
| #bb.metric.visual.sampled-raster-false-positives | Sampled raster false positives | Among R declared reference pixels, finite candidate raster depth where the independently computed analytic six-face reference misses, retaining pixel indices. The current corpus checks every pixel of two named direct-cube fixtures and excludes aggregate, subpixel-point, and inside-bit shortcuts. | Sampled, exhaustive within named frames | Current bounded independent oracle | O(RN), reference rays |
| #bb.metric.visual.sampled-owner-mismatches | Sampled owner mismatches | Among R declared reference pixels, raster owner differs from analytic first hit. | Sampled | Instrument | O(RN), owner buffer and rays |
| #bb.metric.visual.silhouette-holes | Silhouette holes | Background components completely enclosed by a finite-depth foreground component. | Exact | Current | O(P) |
| #bb.metric.visual.shared-edge-cracks | Shared-edge cracks | Reference-covered samples missed along the common edge of adjacent triangles. | Float | Instrument | O(Q) |
| #bb.metric.visual.face-diagonal-seam-mismatches | Face-diagonal seam mismatches | Coverage/depth disagreement along the two-triangle diagonal of one cube face. | Float | Instrument | O(Q) |
| #bb.metric.visual.z-fight-candidates | Z-fight candidates | Fragment contests whose depths differ by less than one declared scale-relative tolerance. | Float | Instrument | O(Q) |
| #bb.metric.visual.static-state-visual-mismatch | Static-state visual mismatch | Pixel/depth entries changed while world, camera, viewport, and overlay inputs are field-equal. | Exact | Instrument | O(P), previous snapshot |
| #bb.metric.visual.invisible-view-state-change | Invisible view-state change | Camera/world/viewport mutation followed by no world-color, depth, or owner-buffer difference. | Exact | Instrument | O(P), previous snapshot |
| #bb.metric.visual.unexplained-visual-change | Unexplained visual change | World-color/depth/owner difference with no field-level view-state or overlay-input change. | Exact | Instrument | O(P), previous snapshot |
| #bb.metric.visual.fit-world-residual | Fit-world residual | Maximum projected bounds-corner excess beyond the central half-pixel immediately after fit. | Float | Current | O(1) |
| #bb.metric.visual.resize-saturation-residual | Resize saturation residual | #bb.metric.visual.fit-world-residual immediately after a top-preserving resize. | Float | Instrument | O(1) |
| #bb.metric.visual.overlay-depth-mutation-violations | Overlay depth-mutation violations | Depth-buffer entries changed by crosshair composition, which should alter RGB only. | Exact | Instrument | O(P), pre-overlay depth |
| #bb.metric.visual.illegal-final-colors | Illegal final colors | Final RGB pixels outside the declared world/background/valid-inversion palette. | Exact | Current | O(P) |
| #bb.metric.visual.point-cube-transition-discontinuity | Point-cube transition discontinuity | Pixel/depth difference across an infinitesimal camera move straddling the one-pixel bit threshold. | Float | Instrument | O(render + P), paired diagnostic renders |
| #bb.metric.visual.aggregate-direct-transition-discontinuity | Aggregate-direct transition discontinuity | Pixel/depth difference across an infinitesimal move straddling top saturation. | Float | Instrument | O(render + P), paired diagnostic renders |
| #bb.metric.visual.one-bit-calibration-residual | One-bit calibration residual | Canonical origin-voxel near-face projection span minus exactly one physical pixel. | Float | Current | O(1) |
| #bb.metric.visual.pixel-center-convention-residual | Pixel-center convention residual | Difference between projection-center convention and raster samples at x+0.5, y+0.5. | Float | Current | O(1) |
| #bb.metric.visual.integer-center-round-trip-failures | Integer center round-trip failures | Occupied coords not recovered by coord_center followed by coord_containing. | Exact | Current | O(N) |
| #bb.metric.visual.face-culling-sign-mismatches | Face-culling sign mismatches | Reference face visibility disagrees with normal-dot-camera culling decision. | Float | Instrument | O(1) per face |
| #bb.metric.visual.triangle-area-coverage-residual | Triangle area-coverage residual | Covered raster samples minus analytically clipped projected triangle area in pixel units. | Float | Instrument | O(Q) |
| #bb.metric.visual.deterministic-tie-break-violations | Deterministic tie-break violations | Ray/sweep ties selecting a coordinate other than the lexicographically first tied coordinate. | Exact | Instrument | O(N) per diagnostic query |
| #bb.metric.visual.impossible-render-state-entries | Impossible render-state entries | Zero-size/empty/inside-bit branches reached contrary to active world and collision invariants. | Exact | Instrument | O(1) per render |
| #bb.metric.visual.crosshair-contrast-violation | Crosshair contrast violation | Crosshair arm sample whose post-inversion contrast against its local neighborhood falls below a declared threshold. | Float | Instrument | O(1) |
| #bb.metric.visual.framebuffer-shape-coherence-violation-vector | Framebuffer shape-coherence violation vector | Checked expected final color length/stride from viewport dimensions versus actual final slice, retaining overflow, length, and stride disagreements. | Exact | Current/Instrument | O(1) |
| #bb.metric.visual.canonical-fragment-winner-violations | Canonical fragment-winner violations | For pixels with fragments tied under the declared finite-depth comparator, contender keys and cases where the stored winner is not least under the declared total tie rule. Distinct from whole-frame iteration permutations and near-ties. | Float | Instrument | O(Q), plus winner witnesses |
| #bb.metric.visual.mapped-surface-copy-mismatch-vector | Mapped-surface copy mismatch vector | After CPU-frame copy and before submission, surface-shape disagreement or every index whose mapped `u32` differs from the final CPU framebuffer, with expected/actual words. Windows now performs one exact complete-slice copy into its owned submission buffer; explicit post-copy mismatch witnesses remain a separate instrument. Exact slice equality is stronger than digest equality. | Exact | Current Windows copy admission; mismatch witnesses Instrument | O(P) and mapped-buffer read |
| #bb.metric.visual.compositor-capture-comparison | Compositor-capture comparison | Opt-in OS screen-raster capture provenance, declared crop/scale/color transform, API/barrier verdict vector, and complete mismatch vector against the referenced submitted framebuffer. The live DisplayReceipt v2 boundary accepts a typed committed composite FrameReceipt v2 and derives the expected framebuffer tag from it. DisplayArtifact v1 binds that receipt to full expected pixels and absent, reused, or distinct captured pixels and recomputes exact observations and comparison facts. The feature-gated F4 path samples the current committed composite once. F5 instead arms the next normally produced successful composite presentation without forging a tick, render, or resubmission; it binds that same exact receipt to its composition-timing envelope, a pre-capture active-output topology observation, one GDI screen capture, and a post-capture topology observation inside PresentationOutput v1. Both paths append to one explicitly fresh typed local journal through checked `sync_all` and exact reread. A nonpoisoning refusal retains the exact complete candidate for an explicit storage-only retry without resampling; a poisoned result requires external typed recovery. Finite samples, direct typed-I/O fault injection, sudden-power survival, latest-prefix authority, external custody, independent TickOutcome and FrameInput ancestry, per-frame DWM attribution, scanout, and physical light remain separate. Exact over retained artifact bytes but only sampled evidence about display. | Sampled | Explicit one-shot Windows GDI screen-DC artifact archive with typed v2 frame provenance, optional timing/topology bundle, and bounded local sync/reread | O(P_capture + routes) plus capture and optional journal replay |
| #bb.metric.visual.physical-display-sensor-comparison | Physical-display sensor comparison | Calibrated external sensor/camera samples mapping requested framebuffer positions/times to measured RGB/luminance residuals, with calibration identity and uncertainty. Without an external instrument this is `N/A`, never pass. | Sampled/Estimated | Future external instrument | O(S) plus calibration |
| #bb.metric.visual.candidate-reference-framebuffer-mismatch-vector | Candidate/reference framebuffer mismatch vector | Shape and named attachment/stage differences between separately executed candidate and reference renderers or compositors for the same declared input, retaining stage, pixel index, and expected/actual XRGB words or `f64` depth bits. The current compositor harness compares complete post-overlay XRGB storage. The bounded independent Python six-face oracle compares all post-overlay XRGB words exactly, all world-pass finite/infinite depth classes exactly, and retains every differing finite depth-bit pair under a declared four-ULP acceptance bound. Passing the corpus is not universal renderer equivalence. | Exact XRGB/classes plus bounded ULP depth over declared corpus | Current compositor harness and bounded independent oracle | O(render_candidate + render_reference + P) per frame |
| #bb.metric.visual.frame-provenance-dependency-cycle-vector | Frame-provenance dependency-cycle vector | Cycles in the declared dependency graph among world/view inputs, metric snapshots, overlay payload, final framebuffer, and frame receipt. Displaying the predecessor frame tag is acyclic; displaying the same frame's final tag is not. | Exact | Current receipts/future sidebar | O(V_dep + E_dep), O(1) for fixed predecessor rule |
| #bb.metric.visual.invalid-xrgb-unused-byte-vector | Invalid XRGB unused-byte vector | Count and pixel-index/word witnesses where a final post-overlay CPU word has any set bit in `0xff00_0000`, before hashing, copy, and present. | Exact | Current facts/Instrument witnesses | O(P) |
| #bb.metric.visual.canonical-frame-encoder-parity-vector | Canonical frame encoder parity vector | Byte-for-byte production versus independent reference encoding over a versioned frame corpus, retaining lengths, first differing byte, and mismatch indices. | Exact over corpus | Instrument | O(corpus bytes) |
| #bb.metric.visual.temporal-framebuffer-delta-vector | Temporal framebuffer delta vector | Exact changed pixel indices, old/new XRGB transition counts, and tight damage rectangle between consecutive submitted CPU frames; dimension changes are explicit. | Exact | Instrument | O(P) and previous frame |
| #bb.metric.visual.crosshair-overlay-rgb-conformance-vector | Crosshair overlay RGB conformance vector | Mismatches against the rule that every distinct in-bounds arm pixel changes by XOR `0x00ff_ffff` exactly once and every other pixel remains unchanged. | Exact | Instrument | O(P) paired slices or O(arms) with write trace |
| #bb.metric.visual.depth-buffer-value-classification-vector | Depth-buffer value classification vector | Counts and witnesses for positive finite, positive zero, negative zero, negative finite, positive infinity, negative infinity, and NaN depth values, with branch-conditioned validity. | Exact over f64 bits | Current/Instrument witnesses | O(P) |
| #bb.metric.visual.sampled-raster-depth-residual-vector | Sampled raster depth residual vector | At declared pixel centers where an independent analytic face oracle and candidate raster agree on finite coverage and face RGB, expected/actual raw `f64` bits plus absolute and ULP entry-depth differences. The current two-fixture corpus retains every nonzero residual and rejects any above four ULP; owner identity remains a separate unimplemented attachment. | Sampled/Float | Current bounded independent oracle | O(RN) |
| #bb.metric.visual.raster-representation-invariance-vector | Raster representation invariance vector | Pixel/depth mismatch vectors after semantics-preserving primitive rewrites including triangle vertex permutations and both diagonals of one projected coplanar quad. | Exact for paired artifacts | Instrument | paired raster cost |
| #bb.metric.visual.render-input-output-binding-verdict | Render input/output binding verdict | Pass/fail/reason that a submitted framebuffer was generated from the exact pinned world, camera bits, viewport, renderer profile, palette, and overlay tags claimed for it; intentional idle reuse is explicit. | Exact if provenance complete | Current frame receipts | verification cost |
| #bb.metric.visual.display-pipeline-state-vector | Display pipeline state vector | Available actual surface, compositor, and output formats; DPI transform; ICC tag; HDR/SDR mode; output bit depth; monitor mode; and visible client region, with unavailable fields explicit. The research-only composition-timing envelope retains a complete null-HWND `DWM_TIMING_INFO` before/after pair, raw HRESULTs, QPC brackets/frequency, and GDI/DWM barrier outcomes around one checked presentation. A bounded Windows active-output observation now retains exact UTF-16 GDI source name; client, monitor, and work rectangles; monitor flags; source position, size, pixel format, and status; every matched clone target route; target identity availability and device path; name/EDID/connector facts; path and reported output technologies; availability, status, rotation, scaling, refresh, signal timing, scanline, path flags, desktop-image facts; and the complete bounded acquisition-attempt transcript or an explicit unavailable outcome. Collector-local HWND, HMONITOR, adapter/source/target IDs, clone IDs, and mode-table indices have no canonical lane. ICC/HDR state, output bit depth, applied transform order, scanout, and physical light remain unavailable. | Exact observed/Host | Current research-only DWM timing and bounded Windows output-topology instrumentation; one retained local F5 sample, not continuous observation | host queries plus O(routes) |
| #bb.metric.visual.raycast-traversal-permutation-divergence | Raycast traversal permutation divergence | Distinct `Hit` outputs and first witness across declared occupied-set traversal permutations for the same immutable world/ray. | Exact over tested permutations | Instrument | O(permutations*N) |
| #bb.metric.visual.presentation-fate-relation | Presentation fate relation | Every backend-accepted frame tag mapped, when independently observable, to displayed, dropped, duplicated, partially scanned, occluded, or unknown output instances with evidence provenance. PresentationOutput v1 retains one exact composite `FrameReceiptV2`, checked presenter facts, session-level DWM timing, one OS screen raster, and before/after active-output topology in one tag-bound object. Full nested-receipt equality, QPC-frequency equality, and capture-not-before-timing-post admission prevent cross-occurrence substitution, while topology stability is only a derived comparison. Modern null-HWND timing still cannot attribute compositor movement to that frame, and topology positions are workflow stages rather than independent timestamps, so the only admissible per-frame fate remains `Unknown`. Counter movement, a matching screen raster, or stable routes never promotes this relation to displayed or dropped. | Host/Sampled | Current Unknown-only checked-presenter/DWM/topology/capture bundle; future independent compositor or sensor attribution | O(presentations + observations + routes) |
| #bb.metric.visual.scanout-composition-witness-vector | Scanout composition witness vector | Independently captured output intervals containing pixels from more than one submitted frame tag, with scan direction, transition line/time, and capture uncertainty. | Sampled/Estimated | Future high-speed capture | capture analysis |
| #bb.metric.visual.display-transform-provenance-relation | Display transform provenance relation | Ordered, content-identified color/spatial transforms from submitted XRGB through compositor, scaling, rotation, color management, tone mapping, and output encoding, with unknown stages explicit. | Host/Audit | Partial GDI audit/future platform instrumentation | O(stages) |
| #bb.metric.visual.interaction-to-display-causal-path | Interaction-to-display causal path | For each traceable interaction, the exact relation among raw input occurrence, normalized accepted-action occurrence, consuming tick, resolved edit/camera state, produced frame, backend submission, and independently observed output instance; elapsed durations are views. InteractionTape v1 begins at the accepted-action boundary, and current headless replay stops at tick/CPU-frame commitments with submission and display lanes explicitly absent. | Mixed exact/Sampled | Partial accepted-action/tick/receipt chain; future raw-input and output linkage | O(events) |
| #bb.metric.visual.raster-floating-point-environment-state | Raster floating-point environment state | Raw render-thread floating-point control state and decoded rounding, precision, flush-to-zero/denormals, exception-mask/status semantics with unsupported fields explicit, sampled before rasterization and after context migration. | Host/Exact observed | Future platform instrumentation | O(1) per sample |
| #bb.metric.view.display-lifecycle-event | Display lifecycle event | Ordered resume, suspend, window-created, surface-created, and surface-dropped event with surface generation, physical size, tick, pending render/present state, and retained-input summary. | Exact/Host | Instrument | O(1) per lifecycle transition |

## Sidebar routing recommendation

The registry is intentionally much larger than one screen. A first sidebar should expose only
stable, cheap primitives: #bb.metric.cam.observer-position, #bb.metric.cam.observer-cell, #bb.metric.cam.yaw, #bb.metric.cam.pitch, #bb.metric.cam.observation-distance, #bb.metric.cam.origin-distance, the rank-1 #bb.metric.aim.ray-hit-record,
#bb.metric.aim.placement-target, #bb.metric.aim.deletion-target, #bb.metric.aim.action-block-reason, #bb.metric.view.top-saturation-state, #bb.metric.view.top-saturation-residual, #bb.metric.view.active-scale-mode, #bb.metric.render.world-pixel-count,
#bb.metric.collision.observer-clear-invariant, #bb.metric.collision.nearest-occupied-record, and active VISUAL invariant failures. Everything else can remain
registered for later sections, toggles, receipts, or offline diagnostics.
