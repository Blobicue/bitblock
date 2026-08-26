# Bitblock world metric registry

This registry owns measurements of the occupied-bit world and its evolution:
sparse/chunk state, geometry, morphology, adjacency and topology, information,
complexity and emergence, temporal dynamics, and multiscale analysis. It does
not own observer/camera/input/render/collision measurements, the tick scheduler,
the text language, replay, proofs, hashes, host performance, disk persistence,
or synchronization.

The registry intentionally records metric *families*. A change of units, an
arbitrary monotone transform, a parameter value, a rolling-window length, or a
different scalar summary of an already-recorded distribution is a view of that
metric, not a new metric. For example, mean/median/quantiles/Gini of the chunk
fill distribution remain views of `#bb.metric.world.chunk-fill-distribution`, and `L1` versus `L2` distance is
a parameter of a distance family unless the two answer materially different
questions.

Full definition tags are immutable. A retired readable registry name is
tombstoned rather than rebound, preserving its historical meaning.

## Notation and labels

- `S ⊂ Z³` is the finite set of occupied coordinates and `N = |S|`.
- `R` is a declared finite region of interest (ROI), with `V = |R|`.
- `G_a` is the occupied-block graph for declared adjacency `a` (`6`, `18`, or
  `26` neighbors).
- `C` is the number of nonempty sparse chunks, `K` the number of spatial
  scales, `T` the retained transition count, and `M` the number of samples or
  intervention trials.
- **Exact** means exactly computable for the finite current state.
- **ROI-exact** means exact only after a finite ROI and boundary convention are
  supplied.
- **Estimated** means sampled, fitted, numerically approximated, or dependent
  on a statistical/model choice. The estimate must carry method and error.
- **History** means exact or estimated only if prior world states/events are
  retained.
- **Rule** means unavailable until an autonomous transition rule exists.
- Costs are rough upper-level collection costs. `E` denotes graph edges and
  `P` the number of changed coordinates in a transition.

## Interpretation guards

1. **The absent-bit universe is infinite.** A global off-bit count is infinite,
   and global density or occupancy entropy is undefined without a finite ROI or
   a precisely stated limiting-window convention. Any such display must name
   its ROI and boundary rule.
2. **A finite snapshot is not a proved fractal.** Every finite occupied set has
   asymptotic box-counting dimension zero. Bitblock may report finite-range
   scaling estimates, but every reported fractal dimension must include its raw
   scale curve, fitted scale interval, number of supported octaves, alternative
   model comparison, fit error, and uncertainty. It must never be labeled proof
   of a hidden fractal.

## Sparse world and chunk state

- **#bb.metric.world.occupied-bit-population — Occupied-bit population.** `N = |S|`.
  **Exactness:** Exact. **Prerequisite:** finite sparse world.
  **Cost:** `O(1)` if maintained, otherwise `O(N)`.
- **#bb.metric.world.mutable-occupied-bit-population — Mutable occupied-bit population.** Occupied bits excluding
  protected world anchors such as bedrock.
  **Exactness:** Exact. **Prerequisite:** block mutability classification.
  **Cost:** `O(1)` if maintained, otherwise `O(N)`.
- **#bb.metric.world.block-class-population-distribution — Block-class population distribution.** Counts by block class;
  today this distinguishes ordinary blocks from bedrock and remains valid if
  more immutable world classes are introduced.
  **Exactness:** Exact. **Prerequisite:** a finite class label on each on bit.
  **Cost:** `O(N)` or `O(1)` with maintained counters.
- **#bb.metric.world.roi-occupancy-measure — ROI occupancy measure.** The pair `(|S∩R|, V)`; off count,
  fill fraction, and percentage are views derived from this pair.
  **Exactness:** ROI-exact. **Prerequisite:** finite `R` and inclusion rule.
  **Cost:** `O(N)` sparse or `O(V)` dense; incrementally maintainable for fixed
  ROIs.
- **#bb.metric.world.coordinate-bounds — Coordinate bounds.** Per-axis minimum and maximum occupied
  coordinates.
  **Exactness:** Exact. **Prerequisite:** nonempty `S`.
  **Cost:** `O(N)`; insertion is `O(1)`, while deletion of an extreme can require
  a rescan without an ordered index.
- **#bb.metric.world.axis-extent-measure — Axis extent measure.** The three inclusive spans and their
  product (AABB cell volume). Aspect ratios and AABB fill are views.
  **Exactness:** Exact. **Prerequisite:** `#bb.metric.world.coordinate-bounds`; products require wide or
  arbitrary-precision arithmetic at full `i64` range.
  **Cost:** `O(1)` from bounds.
- **#bb.metric.world.coordinate-support-distribution — Coordinate-support distribution.** Counts of distinct occupied
  `x`, `y`, and `z` planes, together with population per plane.
  **Exactness:** Exact. **Prerequisite:** nonempty `S`.
  **Cost:** `O(N)` expected with maps.
- **#bb.metric.world.origin-relative-radial-mass-distribution — Origin-relative radial mass distribution.** Population by
  declared integer radius/shell about bedrock origin; `L1`, squared `L2`, and
  `L∞` are parameter views.
  **Exactness:** Exact. **Prerequisite:** origin and distance convention.
  **Cost:** `O(N)`.
- **#bb.metric.world.origin-relative-directional-mass-distribution — Origin-relative directional mass distribution.** Population in
  the eight octants plus explicitly classified coordinate planes/axes.
  **Exactness:** Exact. **Prerequisite:** fixed boundary classification.
  **Cost:** `O(N)`.
- **#bb.metric.world.lattice-residue-distribution — Lattice residue distribution.** Occupancy counts for coordinate
  classes `(x mod m, y mod m, z mod m)` at declared modulus `m`; parity and
  checkerboard imbalance are views at `m=2`.
  **Exactness:** Exact. **Prerequisite:** modulus and signed-remainder convention.
  **Cost:** `O(N)` per requested `m`.
- **#bb.metric.world.nonempty-chunk-population — Nonempty chunk population.** Number of sparse chunks containing
  at least one occupied bit.
  **Exactness:** Exact. **Prerequisite:** declared chunk layout.
  **Cost:** `O(1)` from the sparse map.
- **#bb.metric.world.chunk-coordinate-extent-measure — Chunk-coordinate extent measure.** Bounds and spans of nonempty
  chunk coordinates.
  **Exactness:** Exact. **Prerequisite:** nonempty chunk set.
  **Cost:** `O(C)` or maintained incrementally.
- **#bb.metric.world.chunk-fill-distribution — Chunk fill distribution.** Histogram/multiset of occupied-bit
  counts per nonempty chunk. Utilization statistics are views.
  **Exactness:** Exact. **Prerequisite:** chunk occupancy counts.
  **Cost:** `O(C)`.
- **#bb.metric.world.chunk-pattern-multiplicity-distribution — Chunk-pattern multiplicity distribution.** Multiplicity of each
  distinct in-chunk occupancy bitmap. Unique-pattern count and reuse fraction
  are views of the same distribution.
  **Exactness:** Exact. **Prerequisite:** canonical in-chunk bitmap equality.
  **Cost:** `O(C * chunk_words)` expected using an internal equality index.
- **#bb.metric.world.occupied-chunk-connectivity — Occupied-chunk connectivity.** Component-size distribution of
  nonempty chunks under declared chunk adjacency.
  **Exactness:** Exact. **Prerequisite:** adjacency convention.
  **Cost:** `O(C)` for bounded-degree chunk neighborhoods.
- **#bb.metric.world.chunk-frontier-distribution — Chunk frontier distribution.** Nonempty chunks classified by
  which neighboring chunk directions are empty; the frontier count and
  orientation balance are views.
  **Exactness:** Exact. **Prerequisite:** chunk adjacency convention.
  **Cost:** `O(C)`.
- **#bb.metric.world.chunk-grid-alignment-fragmentation — Chunk-grid alignment fragmentation.** Current nonempty chunk
  count relative to the minimum attainable by translating the fixed-size chunk
  grid through every within-chunk offset.
  **Exactness:** Exact for exhaustively checked offsets. **Prerequisite:** fixed
  chunk size and translation convention.
  **Cost:** `O(chunk_volume * N)`; on-demand.
- **#bb.metric.world.coordinate-control-relation — Coordinate-control relation.** Exact finite relation from each
  occupied mutable coordinate to its controlling key or policy tag (or
  explicitly unowned) at an effective receipt boundary. Owner counts and
  balances are views of this relation.
  **Exactness:** History-exact under the declared authorization protocol.
  **Prerequisite:** tagged ownership/policy state and complete admitted history.
  **Cost:** `O(N)` rebuild or maintained `O(1)` per transfer.

## Geometry and morphology

- **#bb.metric.geom.centroid — Centroid.** `mu = (1/N) sum(p)` over occupied coordinates.
  **Exactness:** Exact as rational coordinate sums; floating display is rounded.
  **Prerequisite:** `N>0`. **Cost:** `O(N)` or `O(1)` from maintained moments.
- **#bb.metric.geom.medoid — Medoid.** Occupied coordinate minimizing total declared distance
  to all occupied coordinates.
  **Exactness:** Exact when exhaustively minimized. **Prerequisite:** distance
  norm. **Cost:** `O(N^2)` naive; estimated for large worlds.
- **#bb.metric.geom.geometric-median — Geometric median.** Continuous point minimizing total Euclidean
  distance to occupied coordinates.
  **Exactness:** Estimated numerically to tolerance. **Prerequisite:** `N>0` and
  numeric tolerance. **Cost:** `O(iterations * N)`.
- **#bb.metric.geom.second-moment-shape-tensor — Second-moment shape tensor.** Coordinate covariance/inertia
  tensor about the centroid. Principal axes, eigenvalue spectrum, linearity,
  planarity, and volumetricity are views.
  **Exactness:** Exact moments; eigenvectors are numerical estimates.
  **Prerequisite:** `#bb.metric.geom.centroid`. **Cost:** `O(N)` plus constant-size eigensolve.
- **#bb.metric.geom.radius-of-gyration — Radius of gyration.** Root-mean-square distance from centroid.
  **Exactness:** Exact squared value/rational moment; displayed root is numeric.
  **Prerequisite:** `#bb.metric.geom.centroid`. **Cost:** `O(N)` or maintained moments.
- **#bb.metric.geom.minimum-enclosing-ball — Minimum enclosing ball.** Center and radius of the smallest ball
  containing all occupied voxel centers or voxel cubes, with representation
  convention stored.
  **Exactness:** Exact in principle; floating computational geometry is numeric.
  **Prerequisite:** center-versus-solid-voxel convention.
  **Cost:** expected near `O(N)` with a specialized algorithm; on-demand.
- **#bb.metric.geom.diameter-distribution-family — Diameter distribution family.** Maximum occupied-pair separation
  and the orientation(s) attaining it under a declared norm.
  **Exactness:** Exact with exhaustive or proven geometric algorithm.
  **Prerequisite:** norm and center-versus-cube convention.
  **Cost:** `O(N^2)` naive; hull/extreme-point reductions may apply.
- **#bb.metric.geom.pair-distance-distribution — Pair-distance distribution.** Multiset/histogram of distances
  between all unordered occupied pairs.
  **Exactness:** Exact if all pairs are enumerated; otherwise Estimated with pair
  sampling error. **Prerequisite:** norm and binning for histogram view.
  **Cost:** `O(N^2)` exact.
- **#bb.metric.geom.order-k-neighbor-distance-profile — Order-k neighbor distance profile.** Distance to the `k`th
  nearest occupied coordinate as a parameterized family.
  **Exactness:** Exact with exact neighbor queries. **Prerequisite:** `k` and norm.
  **Cost:** about `O(N log N + kN)`.
- **#bb.metric.geom.euclidean-minimum-spanning-tree-morphology — Euclidean minimum-spanning-tree morphology.** Edge-length and
  branch-degree distributions of the point-set MST; total length is a view.
  **Exactness:** Exact with a proven MST construction. **Prerequisite:** point
  representation convention. **Cost:** `O(N log N)` specialized, `O(N^2)` naive.
- **#bb.metric.geom.rectilinear-steiner-connection-length — Rectilinear Steiner connection length.** Minimum `L1` network
  length connecting all occupied coordinates, allowing new junction points.
  **Exactness:** Exact only for tractable instances; otherwise Estimated with a
  bound. **Prerequisite:** rectilinear geometry convention.
  **Cost:** NP-hard; on-demand approximation.
- **#bb.metric.geom.axis-projection-measure — Axis projection measure.** Occupied area/count in each of the
  three axis-aligned projections.
  **Exactness:** Exact. **Prerequisite:** projection convention.
  **Cost:** `O(N)` expected.
- **#bb.metric.geom.column-span-distribution — Column-span distribution.** Inclusive occupied span along every
  nonempty axis-aligned column.
  **Exactness:** Exact. **Prerequisite:** selected column axis.
  **Cost:** `O(N)` with per-column extrema.
- **#bb.metric.geom.column-gap-distribution — Column-gap distribution.** Counts and lengths of off runs between
  occupied cells on occupied lattice lines.
  **Exactness:** Exact. **Prerequisite:** selected line directions.
  **Cost:** `O(N log N)` through sorted line coordinates.
- **#bb.metric.geom.occupied-chord-distribution — Occupied chord distribution.** Lengths of maximal contiguous on
  runs along declared lattice directions.
  **Exactness:** Exact. **Prerequisite:** direction set and adjacency along line.
  **Cost:** `O(N log N)` or `O(N)` with indexed neighbors.
- **#bb.metric.geom.line-intercept-distribution — Line-intercept distribution.** Number of disjoint occupied
  segments intersected by each declared lattice line.
  **Exactness:** Exact. **Prerequisite:** direction set.
  **Cost:** same traversal as `#bb.metric.geom.occupied-chord-distribution`.
- **#bb.metric.geom.exposed-face-area — Exposed-face area.** Number of occupied voxel faces whose
  face-neighbor is off.
  **Exactness:** Exact. **Prerequisite:** solid unit-cube interpretation.
  **Cost:** `O(N)` expected.
- **#bb.metric.geom.exterior-accessible-surface-area — Exterior-accessible surface area.** Exposed faces connected to
  the unbounded exterior, excluding sealed cavities.
  **Exactness:** Exact under a finite padded-domain/exterior convention.
  **Prerequisite:** off-cell adjacency and exterior classification.
  **Cost:** naive `O(V_AABB)` flood fill; sparse coordinate compression possible.
- **#bb.metric.geom.enclosed-cavity-surface-area — Enclosed-cavity surface area.** Exposed faces bounding bounded
  off-components. This is not inferred merely from total surface when exterior
  classification is unavailable.
  **Exactness:** ROI-exact/exact with a proven exterior boundary.
  **Prerequisite:** same as `#bb.metric.geom.exterior-accessible-surface-area`. **Cost:** same as exterior flood fill.
- **#bb.metric.geom.boundary-block-distribution — Boundary-block distribution.** Occupied voxels classified by
  their local exposed-face mask. Boundary count and exposure-degree histogram
  are views.
  **Exactness:** Exact. **Prerequisite:** face-neighbor convention.
  **Cost:** `O(N)`.
- **#bb.metric.geom.surface-normal-distribution — Surface-normal distribution.** Exposed area by `+/-x`, `+/-y`,
  and `+/-z`; tensor and anisotropy summaries are views.
  **Exactness:** Exact. **Prerequisite:** `#bb.metric.geom.exposed-face-area` face classification.
  **Cost:** `O(N)`.
- **#bb.metric.geom.contact-type-distribution — Contact-type distribution.** Counts of occupied pairs sharing a
  face, an edge only, or a corner only.
  **Exactness:** Exact. **Prerequisite:** unit-cube interpretation.
  **Cost:** `O(N)` with bounded neighbor probes.
- **#bb.metric.geom.convex-hull-measure — Convex-hull measure.** Hull vertices/faces, volume, and surface
  area for occupied centers or full voxel cubes. Solidity and convex deficit are
  views derived from hull and occupied volume.
  **Exactness:** Exact in exact arithmetic; usually numerical computational
  geometry. **Prerequisite:** center-versus-solid convention.
  **Cost:** roughly `O(N log N)` plus hull-size-dependent work.
- **#bb.metric.geom.minimum-oriented-bounding-box — Minimum oriented bounding box.** Minimum-volume rotated box and
  its dimensions/orientation.
  **Exactness:** Exact in principle over hull candidates; numeric in practice.
  **Prerequisite:** `#bb.metric.geom.convex-hull-measure`. **Cost:** hull-dependent and on-demand.
- **#bb.metric.geom.interior-thickness-distribution — Interior thickness distribution.** Distance from occupied
  interior samples to the nearest exterior, under a named metric; maximum
  inscribed radius is a view.
  **Exactness:** ROI-exact/exact under an exterior convention.
  **Prerequisite:** solid voxel volume and distance transform.
  **Cost:** `O(V)` dense or sparse-distance-transform dependent.
- **#bb.metric.geom.empty-space-clearance-distribution — Empty-space clearance distribution.** Distance from off cells in
  declared cavities/ROI to the nearest occupied solid; pore radius is a view.
  **Exactness:** ROI-exact. **Prerequisite:** finite ROI and distance convention.
  **Cost:** `O(V)` dense.
- **#bb.metric.geom.mean-breadth — Mean breadth.** The third 3D Minkowski functional, equivalently a
  normalized integrated mean curvature derived from cubical incidence counts.
  **Exactness:** Exact for the cubical complex. **Prerequisite:** closed-cube
  incidence convention. **Cost:** `O(N)`.
- **#bb.metric.geom.local-discrete-curvature-distribution — Local discrete-curvature distribution.** Boundary vertex/edge
  curvature contributions rather than only their integrated total.
  **Exactness:** Exact for a named cubical curvature convention.
  **Prerequisite:** boundary complex. **Cost:** `O(N)`.
- **#bb.metric.geom.surface-roughness-measure — Surface roughness measure.** Excess exterior area relative to a
  declared hull/smoothed reference; ratio and absolute excess are views.
  **Exactness:** Exact once both surfaces are exact; otherwise Estimated.
  **Prerequisite:** `#bb.metric.geom.exterior-accessible-surface-area` and named reference surface.
  **Cost:** dominated by exterior/reference construction.
- **#bb.metric.geom.height-field-roughness-distribution — Height-field roughness distribution.** Height variance, gradient,
  and structure over a named visible/outer surface projection; these are views
  of one sampled height field.
  **Exactness:** ROI-exact for the discrete height field.
  **Prerequisite:** projection direction and overhang convention.
  **Cost:** `O(N + projection_area)`.
- **#bb.metric.geom.contact-fabric-tensor — Contact fabric tensor.** Second angular moment of occupied
  neighbor/contact directions.
  **Exactness:** Exact moments; eigen summary numeric.
  **Prerequisite:** contact definition. **Cost:** `O(N)`.
- **#bb.metric.geom.directional-support-profile — Directional support profile.** Minimum/maximum support and Feret
  width over a declared direction set.
  **Exactness:** Exact for the finite direction set; Estimated for a continuous
  directional optimum. **Prerequisite:** directions and point/cube convention.
  **Cost:** `O(direction_count * N)`.
- **#bb.metric.geom.exact-rigid-symmetry-group — Exact rigid-symmetry group.** Set/order of lattice rotations and
  reflections about a declared center that preserve `S` exactly.
  **Exactness:** Exact over the finite cubic isometry group.
  **Prerequisite:** center and whether reflections are admitted.
  **Cost:** `O(48N)` for full cubic rotations/reflections.
- **#bb.metric.geom.approximate-symmetry-defect — Approximate symmetry defect.** Minimum normalized symmetric
  difference under a declared finite transform family.
  **Exactness:** Exact over exhaustive candidate transforms; otherwise Estimated.
  **Prerequisite:** transform family and normalization.
  **Cost:** `O(candidate_count * N)` expected-set comparisons.
- **#bb.metric.geom.chirality-defect — Chirality defect.** Best permitted alignment mismatch between a
  shape and its mirror image.
  **Exactness:** Exact only for exhaustive finite lattice alignments; otherwise
  Estimated. **Prerequisite:** alignment family. **Cost:** potentially high.
- **#bb.metric.geom.point-to-shape-visibility-distribution — Point-to-shape visibility distribution.** Fraction/mask of
  occupied blocks visible by an unobstructed segment from a declared world
  coordinate.
  **Exactness:** Exact under a voxel segment-intersection convention.
  **Prerequisite:** reference point and ray boundary convention.
  **Cost:** `O(N * ray_cost)`.
- **#bb.metric.geom.visibility-kernel-measure — Visibility-kernel measure.** Set/volume of world locations from
  which the entire shape is visible.
  **Exactness:** ROI-exact for enumerated candidate cells; continuous form is
  computational geometry. **Prerequisite:** finite candidate ROI.
  **Cost:** up to `O(VN * ray_cost)`; on-demand/estimated.
- **#bb.metric.geom.pairwise-visibility-distribution — Pairwise visibility distribution.** Unobstructed-line relation
  among occupied pairs; visibility fraction and degree distribution are views.
  **Exactness:** Exact if all pairs are tested. **Prerequisite:** segment/voxel
  boundary convention. **Cost:** `O(N^2 * ray_cost)`; normally sampled.
- **#bb.metric.geom.geodesic-tortuosity-distribution — Geodesic tortuosity distribution.** Ratio of shortest occupied
  path length to direct geometric displacement for declared connected pairs.
  **Exactness:** Exact after exact graph distances; otherwise Estimated by pair
  sampling. **Prerequisite:** adjacency and distance units.
  **Cost:** all-pairs graph cost or sampled BFS.
- **#bb.metric.geom.moment-invariant-shape-descriptor — Moment-invariant shape descriptor.** Translation/rotation/scale
  normalized 3D moments or spherical/Zernike coefficients up to declared order.
  **Exactness:** Exact moments with numerical basis evaluation.
  **Prerequisite:** normalization and basis/order.
  **Cost:** `O(N * coefficient_count)`.
- **#bb.metric.geom.hausdorff-shape-distance — Hausdorff shape distance.** Maximum bidirectional nearest-point
  mismatch between two snapshots/components after a declared alignment.
  **Exactness:** Exact with exact nearest-neighbor and alignment search; otherwise
  Estimated. **Prerequisite:** comparison shape and transform family.
  **Cost:** about `O(N log N)` per alignment candidate.
- **#bb.metric.geom.chamfer-shape-distance — Chamfer shape distance.** Mean bidirectional nearest-point
  mismatch, retaining sensitivity distinct from Hausdorff's worst case.
  **Exactness:** Exact for fixed alignment and exact neighbor queries.
  **Prerequisite:** comparison shape and transform family.
  **Cost:** about `O(N log N)` per candidate.
- **#bb.metric.geom.occupancy-transport-distance — Occupancy transport distance.** Minimum cost to transport one
  normalized occupancy measure into another.
  **Exactness:** Exact in principle; generally Estimated by transport solvers.
  **Prerequisite:** two finite measures, ground metric, alignment convention.
  **Cost:** high polynomial exact; approximate on-demand.

## Adjacency graph and topology

Every entry in this section records its adjacency convention. Values for
`G_6`, `G_18`, and `G_26` are parameter views of the same family, not three new
metrics. Structurally constant facts are not useful metrics; for example,
`G_6` is always bipartite.

- **#bb.metric.topo.adjacency-edge-population — Adjacency-edge population.** Number of occupied-neighbor pairs in
  `G_a`.
  **Exactness:** Exact. **Prerequisite:** adjacency `a`.
  **Cost:** `O(N)` bounded neighbor probes.
- **#bb.metric.topo.connected-component-size-distribution — Connected-component size distribution.** Multiset of occupied
  component sizes; component count, largest share, and isolated-block count are
  views.
  **Exactness:** Exact. **Prerequisite:** adjacency `a`.
  **Cost:** `O(N+E)`.
- **#bb.metric.topo.origin-component-measure — Origin-component measure.** Membership and size/distance profile
  of the component containing bedrock origin.
  **Exactness:** Exact. **Prerequisite:** occupied origin and adjacency `a`.
  **Cost:** `O(N+E)` or incremental connectivity support.
- **#bb.metric.topo.directional-spanning-relation — Directional spanning relation.** Which occupied components touch
  both opposing faces of the AABB or a declared ROI on each axis.
  **Exactness:** Exact for AABB; ROI-exact for arbitrary `R`.
  **Prerequisite:** boundary and touch convention. **Cost:** `O(N+E)`.
- **#bb.metric.topo.degree-distribution — Degree distribution.** Multiset/histogram of vertex degrees in
  `G_a`; leaf and junction populations are threshold views.
  **Exactness:** Exact. **Prerequisite:** adjacency `a`.
  **Cost:** `O(N+E)`.
- **#bb.metric.topo.core-decomposition — Core decomposition.** Coreness of every block and component;
  degeneracy and `k`-core sizes are views.
  **Exactness:** Exact. **Prerequisite:** `G_a`.
  **Cost:** `O(N+E)`.
- **#bb.metric.topo.articulation-profile — Articulation profile.** Blocks whose removal increases component
  count, including the split-size distribution caused by each removal.
  **Exactness:** Exact. **Prerequisite:** `G_a`.
  **Cost:** `O(N+E)` for points; split-size annotations add bookkeeping.
- **#bb.metric.topo.bridge-profile — Bridge profile.** Edges whose removal increases component count,
  including the separated-side size for each bridge.
  **Exactness:** Exact. **Prerequisite:** `G_a`.
  **Cost:** `O(N+E)`.
- **#bb.metric.topo.biconnected-decomposition — Biconnected decomposition.** Vertex-biconnected and
  edge-biconnected block-size distributions.
  **Exactness:** Exact. **Prerequisite:** `G_a` and selected decomposition type.
  **Cost:** `O(N+E)`.
- **#bb.metric.topo.vertex-connectivity — Vertex connectivity.** Minimum number of occupied vertices whose
  removal disconnects a selected component.
  **Exactness:** Exact with exhaustive polynomial connectivity algorithms.
  **Prerequisite:** selected connected component. **Cost:** expensive repeated
  flow/min-cut work; on-demand.
- **#bb.metric.topo.edge-connectivity — Edge connectivity.** Minimum number of adjacency edges whose
  removal disconnects a selected component.
  **Exactness:** Exact. **Prerequisite:** selected connected component.
  **Cost:** repeated min-cut/flow; on-demand.
- **#bb.metric.topo.graph-cycle-rank — Graph cycle rank.** Independent graph-cycle count
  `E - N + component_count`.
  **Exactness:** Exact. **Prerequisite:** `#bb.metric.topo.adjacency-edge-population` and `#bb.metric.topo.connected-component-size-distribution`.
  **Cost:** `O(1)` after those measures.
- **#bb.metric.topo.simple-cycle-spectrum — Simple-cycle spectrum.** Counts of simple cycles by length and
  component. Bounded maximum length is a parameter, not another metric.
  **Exactness:** Exact only when the requested search is exhausted; otherwise
  Estimated. **Prerequisite:** `G_a` and length policy.
  **Cost:** #P/exponential in general.
- **#bb.metric.topo.local-graph-motif-census — Local graph-motif census.** Counts of declared small induced or
  non-induced adjacency patterns (paths, stars, plaquettes, cube subgraphs).
  **Exactness:** Exact for a finite motif dictionary.
  **Prerequisite:** motif dictionary and induced/non-induced convention.
  **Cost:** `O(N * motif_search_cost)`.
- **#bb.metric.topo.clique-distribution — Clique distribution.** Counts/maximal sizes of complete
  occupied-neighbor subgraphs; mostly structurally trivial for `G_6`.
  **Exactness:** Exact when enumeration finishes. **Prerequisite:** `G_a`.
  **Cost:** exponential worst case; bounded on lattice degree.
- **#bb.metric.topo.shortest-path-distance-distribution — Shortest-path distance distribution.** Multiset of finite graph
  distances over connected vertex pairs.
  **Exactness:** Exact for all-pairs traversal; otherwise Estimated by sampled
  sources/pairs. **Prerequisite:** `G_a`.
  **Cost:** `O(N(N+E))` unweighted exact.
- **#bb.metric.topo.eccentricity-distribution — Eccentricity distribution.** Maximum within-component graph
  distance from each vertex; graph radius and diameter are views.
  **Exactness:** Exact with all-source traversal. **Prerequisite:** `G_a`.
  **Cost:** `O(N(N+E))`, usually sampled/approximated.
- **#bb.metric.topo.local-efficiency-distribution — Local efficiency distribution.** Efficiency of each vertex's
  induced neighbor subgraph.
  **Exactness:** Exact. **Prerequisite:** `G_a`.
  **Cost:** repeated bounded-neighborhood traversals, roughly `O(N)` for fixed
  lattice degree.
- **#bb.metric.topo.triangle-closure-distribution — Triangle closure distribution.** Local clustering and global
  transitivity derived from triangle/wedge incidence.
  **Exactness:** Exact. **Prerequisite:** adjacency supporting triangles; it is
  identically zero for `G_6`. **Cost:** `O(E * max_degree)`.
- **#bb.metric.topo.degree-assortativity — Degree assortativity.** Correlation/dependence of endpoint degrees
  across adjacency edges.
  **Exactness:** Exact rational moments; displayed correlation is numeric.
  **Prerequisite:** `#bb.metric.topo.degree-distribution`. **Cost:** `O(E)`.
- **#bb.metric.topo.betweenness-centrality-distribution — Betweenness-centrality distribution.** Fraction/count of shortest
  paths passing through each occupied vertex.
  **Exactness:** Exact with all sources; otherwise Estimated.
  **Prerequisite:** `G_a`. **Cost:** `O(NE)` with unweighted Brandes.
- **#bb.metric.topo.closeness-centrality-distribution — Closeness-centrality distribution.** Reciprocal total graph
  distance from each vertex, with disconnected handling named.
  **Exactness:** Exact with all-source distances; otherwise Estimated.
  **Prerequisite:** `G_a`. **Cost:** `O(N(N+E))` exact.
- **#bb.metric.topo.eigenvector-centrality-distribution — Eigenvector-centrality distribution.** Principal adjacency
  eigenvector weights, with component/normalization convention recorded.
  **Exactness:** Numerically Estimated to tolerance. **Prerequisite:** `G_a`.
  **Cost:** about `O(iterations * E)` sparse.
- **#bb.metric.topo.adjacency-spectral-measure — Adjacency spectral measure.** Adjacency eigenvalue distribution;
  spectral radius and graph energy are views.
  **Exactness:** Exact algebraically, normally numerical to tolerance.
  **Prerequisite:** `G_a`. **Cost:** up to `O(N^3)` full dense spectrum; partial
  sparse estimates are cheaper.
- **#bb.metric.topo.laplacian-spectral-measure — Laplacian spectral measure.** Combinatorial or normalized
  Laplacian eigenvalue distribution. Algebraic connectivity and mixing-gap
  summaries are views.
  **Exactness:** Exact algebraically, normally numerical to tolerance.
  **Prerequisite:** selected Laplacian convention. **Cost:** full spectrum up to
  `O(N^3)`; selected sparse eigenvalues iterative.
- **#bb.metric.topo.spanning-tree-population — Spanning-tree population.** Number of spanning trees in each
  connected component.
  **Exactness:** Exact integer through Kirchhoff determinants.
  **Prerequisite:** component graph. **Cost:** about `O(N^3)` dense arithmetic.
- **#bb.metric.topo.effective-resistance-distribution — Effective-resistance distribution.** Electrical-network
  resistance over connected vertex pairs; Kirchhoff index is a view.
  **Exactness:** Exact rational in principle, normally numerical.
  **Prerequisite:** unit or declared edge conductances.
  **Cost:** Laplacian solves/full pseudoinverse; expensive.
- **#bb.metric.topo.conductance-profile — Conductance profile.** Minimum boundary-edge/volume ratio as a
  function of subset size/balance; Cheeger constant is a view.
  **Exactness:** Exact definition but hard optimization; generally Estimated.
  **Prerequisite:** graph volume and cut convention. **Cost:** combinatorial/NP-
  hard variants; spectral or heuristic bounds.
- **#bb.metric.topo.community-structure — Community structure.** Best-found modularity and partition-size
  distribution under a named community objective.
  **Exactness:** Estimated because optimization/algorithm is model-dependent.
  **Prerequisite:** objective, resolution, and algorithm.
  **Cost:** algorithm-dependent, commonly near `O(E)` per pass.
- **#bb.metric.topo.graph-automorphism-group — Graph automorphism group.** Number/structure of vertex
  permutations preserving adjacency, independent of geometric embedding.
  **Exactness:** Exact when graph-automorphism search completes.
  **Prerequisite:** `G_a`. **Cost:** graph-isomorphism style; on-demand.
- **#bb.metric.topo.gromov-hyperbolicity — Gromov hyperbolicity.** Minimum `delta` satisfying the graph
  four-point condition; sample distribution is a view when not exhaustive.
  **Exactness:** Exact with all quadruples, otherwise Estimated.
  **Prerequisite:** graph distance. **Cost:** `O(N^4)` naive.
- **#bb.metric.topo.tree-decomposition-width — Tree decomposition width.** Treewidth and an attaining/bounding
  decomposition.
  **Exactness:** Exact only with exhaustive/parameterized solve; otherwise bounds.
  **Prerequisite:** `G_a`. **Cost:** NP-hard; on-demand.
- **#bb.metric.topo.balanced-separator-profile — Balanced-separator profile.** Minimum vertex/edge separator size
  as a function of required partition balance.
  **Exactness:** Exact in principle, generally Estimated/bounded.
  **Prerequisite:** balance and separator convention. **Cost:** hard optimization.
- **#bb.metric.topo.maximum-matching-measure — Maximum matching measure.** Maximum number of pairwise
  vertex-disjoint adjacency edges and unmatched-vertex distribution.
  **Exactness:** Exact. **Prerequisite:** `G_a`.
  **Cost:** polynomial matching algorithm.
- **#bb.metric.topo.dominating-set-measure — Dominating-set measure.** Minimum occupied subset adjacent to or
  containing every occupied vertex.
  **Exactness:** Exact in principle; usually Estimated/bounded.
  **Prerequisite:** open/closed-neighborhood convention. **Cost:** NP-hard.
- **#bb.metric.topo.random-walk-return-distribution — Random-walk return distribution.** Return probability versus walk
  length, averaged over a declared start distribution.
  **Exactness:** Numerical from transition matrices or Estimated by walks.
  **Prerequisite:** walk and start conventions. **Cost:** sparse matrix powers or
  `O(M * walk_length)`.
- **#bb.metric.topo.random-walk-first-passage-distribution — Random-walk first-passage distribution.** Hitting-time
  distribution/expectation between declared start and target ensembles.
  **Exactness:** Numerical solve or Estimated by simulation.
  **Prerequisite:** walk, starts, targets. **Cost:** linear solves or sampled walks.
- **#bb.metric.topo.random-walk-cover-time-distribution — Random-walk cover-time distribution.** Time required to visit all
  vertices of a selected component.
  **Exactness:** Generally Estimated by simulation.
  **Prerequisite:** selected component and start distribution.
  **Cost:** `O(M * realized_walk_length)`.
- **#bb.metric.topo.random-walk-mixing-measure — Random-walk mixing measure.** Distance to stationary distribution
  versus walk length; mixing time is a threshold view.
  **Exactness:** Numerical/Estimated. **Prerequisite:** connected nonperiodic walk
  convention. **Cost:** spectral or repeated sparse transitions.
- **#bb.metric.topo.cubical-complex-incidence-measure — Cubical-complex incidence measure.** Counts of unique cubical
  vertices, edges, faces, and cubes. Euler characteristic is the alternating-sum
  view and is not a separate metric definition.
  **Exactness:** Exact. **Prerequisite:** closed-unit-cube incidence convention.
  **Cost:** `O(N)` expected with deduplicated incidences.
- **#bb.metric.topo.cubical-homology-measure — Cubical homology measure.** Betti numbers and representative
  cycles for the occupied cubical complex. `beta_0` must agree with the matching
  component convention; `beta_1` measures tunnels and `beta_2` enclosed voids.
  **Exactness:** Exact over the chosen coefficient field.
  **Prerequisite:** cubical complex and coefficient field.
  **Cost:** sparse boundary-matrix reduction, worst-case cubic.
- **#bb.metric.topo.cavity-volume-distribution — Cavity-volume distribution.** Volumes of bounded off-components
  enclosed by occupied cubes.
  **Exactness:** ROI-exact/exact under a padded exterior convention.
  **Prerequisite:** off-cell adjacency and finite exploration boundary.
  **Cost:** naive `O(V_AABB)` flood fill.
- **#bb.metric.topo.boundary-topology-distribution — Boundary topology distribution.** Connected boundary components,
  orientability/manifold defects, and genus per valid closed component.
  **Exactness:** Exact under a named boundary-complex convention.
  **Prerequisite:** extracted boundary complex. **Cost:** linear in boundary size
  plus homology checks.
- **#bb.metric.topo.roi-complement-connectivity — ROI complement connectivity.** Component-size distribution of
  off cells inside declared `R`, explicitly distinguishing exterior-touching and
  enclosed components.
  **Exactness:** ROI-exact. **Prerequisite:** finite `R` and off-cell adjacency.
  **Cost:** `O(V)` dense.
- **#bb.metric.topo.reeb-contour-graph — Reeb/contour graph.** Connectivity graph of level sets under a
  declared scalar function such as coordinate height or distance transform.
  **Exactness:** Exact for the discrete filtration and tie rule.
  **Prerequisite:** scalar field and tie-breaking convention.
  **Cost:** sorting plus complex traversal, roughly `O(N log N)` or higher.
- **#bb.metric.topo.discrete-morse-critical-cell-distribution — Discrete Morse critical-cell distribution.** Critical cells and
  topology-changing events under a declared filtration.
  **Exactness:** Exact relative to the discrete Morse matching.
  **Prerequisite:** cubical complex, filtration, matching rule.
  **Cost:** algorithm-dependent, at least linear in complex size.
- **#bb.metric.topo.topology-preserving-skeleton — Topology-preserving skeleton.** Skeleton voxel/edge distribution
  and branch structure under a named thinning or medial-axis algorithm.
  **Exactness:** Exact relative to the deterministic algorithm, not unique across
  algorithms. **Prerequisite:** algorithm and connectivity pair.
  **Cost:** `O(V)` dense or sparse algorithm dependent.
- **#bb.metric.topo.persistent-homology-barcode — Persistent-homology barcode.** Birth/death intervals for
  components, tunnels, and voids under a named geometric/height/dilation
  filtration.
  **Exactness:** Exact for a finite filtration and coefficient field.
  **Prerequisite:** filtration, tie rule, coefficient field.
  **Cost:** boundary-matrix reduction, worst-case cubic.
- **#bb.metric.topo.component-merge-tree — Component merge tree.** Exact hierarchy and scale/threshold at
  which separate components merge under a declared filtration.
  **Exactness:** Exact. **Prerequisite:** monotone filtration.
  **Cost:** sorting/union-find, commonly near `O(complex_size log complex_size)`.
- **#bb.metric.topo.random-deletion-connectivity-response — Random-deletion connectivity response.** Component/giant-share
  curve when occupied blocks or adjacency edges are independently removed.
  **Exactness:** Estimated over random trials; exhaustive only for tiny worlds.
  **Prerequisite:** deletion ensemble and fractions. **Cost:** `O(M(N+E))` per
  sampled fraction schedule.
- **#bb.metric.topo.targeted-deletion-connectivity-response — Targeted-deletion connectivity response.** Connectivity curve
  under a declared deterministic removal order, such as degree or betweenness.
  **Exactness:** Exact for the fixed order; the order metric may be Estimated.
  **Prerequisite:** removal policy. **Cost:** repeated connectivity and possibly
  centrality recomputation.
- **#bb.metric.topo.occupancy-percolation-response — Occupancy percolation response.** Probability/distribution of
  spanning connectivity under declared randomized filling or thinning.
  **Exactness:** Estimated over an ensemble. **Prerequisite:** finite ROI,
  boundary/spanning rule, probability model. **Cost:** `O(MV)` per density grid.
- **#bb.metric.topo.graph-communicability-distribution — Graph communicability distribution.** Walk-weighted connection
  strength, conventionally entries of `exp(A)` or a named resolvent.
  **Exactness:** Numerical/Estimated to tolerance. **Prerequisite:** kernel and
  graph normalization. **Cost:** spectral/Krylov methods; potentially `O(N^3)`.

## Information and description complexity

A deterministic snapshot has no unique Shannon entropy by itself. Each entropy
entry therefore names the ensemble being sampled: ROI cells, patches, lines,
chunks, components, or a retained temporal process.

- **#bb.metric.info.roi-occupancy-entropy-spectrum — ROI occupancy entropy spectrum.** Rényi entropy `H_q` of the
  on/off cell distribution in `R`; Shannon, order-2, and min entropy are
  parameter views at `q=1`, `q=2`, and the `q -> infinity` limit.
  **Exactness:** ROI-exact from exact counts, with numerical logarithms.
  **Prerequisite:** finite `R`, sampling weights, and `q`.
  **Cost:** `O(1)` after `#bb.metric.world.roi-occupancy-measure`.
- **#bb.metric.info.spatial-patch-entropy — Spatial patch entropy.** Entropy of all occupancy patterns in
  declared finite patch shape/size sampled over `R`.
  **Exactness:** ROI-exact when all valid placements are counted; Estimated when
  sampled. **Prerequisite:** finite `R`, patch shape, boundary treatment.
  **Cost:** `O(V * patch_volume)` naive; rolling encodings can reduce it.
- **#bb.metric.info.spatial-entropy-rate-estimate — Spatial entropy-rate estimate.** Incremental information per new
  cell inferred from growth of patch entropy under a declared scan or nested
  patch family.
  **Exactness:** Estimated; a finite snapshot cannot establish the limiting
  rate. **Prerequisite:** several patch sizes and stationarity assumption.
  **Cost:** sum of `#bb.metric.info.spatial-patch-entropy` costs across sizes plus fitting.
- **#bb.metric.info.spatial-excess-information — Spatial excess information.** Subextensive/predictive information
  remaining after the fitted extensive entropy-rate term is removed from block
  entropy growth.
  **Exactness:** Estimated. **Prerequisite:** `#bb.metric.info.spatial-entropy-rate-estimate`, fit model/range.
  **Cost:** negligible after patch-entropy curve.
- **#bb.metric.info.neighborhood-pattern-entropy — Neighborhood-pattern entropy.** Entropy of finite on/off
  configurations around sampled sites under a declared neighborhood.
  **Exactness:** ROI-exact for exhaustive placements; Estimated when sampled.
  **Prerequisite:** neighborhood, center inclusion, boundary rule.
  **Cost:** `O(V * neighborhood_size)` naive.
- **#bb.metric.info.conditional-local-uncertainty — Conditional local uncertainty.** Conditional entropy of the
  center bit given its declared neighboring bits.
  **Exactness:** ROI-exact from exhaustive pattern counts; otherwise Estimated.
  **Prerequisite:** `#bb.metric.info.neighborhood-pattern-entropy` joint counts. **Cost:** `O(pattern_count)` after
  collection.
- **#bb.metric.info.displacement-mutual-information-field — Displacement mutual-information field.** Mutual information
  between occupancy at `p` and `p+d` for displacement vector `d`; radial and
  directional profiles are views.
  **Exactness:** ROI-exact for all valid pairs; Estimated when sampled.
  **Prerequisite:** finite `R`, displacement set, edge correction.
  **Cost:** `O(V * displacement_count)`.
- **#bb.metric.info.multivariate-neighborhood-dependence — Multivariate neighborhood dependence.** Joint-dependence family
  for a declared neighborhood: total correlation, dual total correlation, and
  connected information by order are named views of the same joint
  distribution.
  **Exactness:** ROI-exact from exact pattern probabilities; otherwise Estimated.
  **Prerequisite:** joint pattern distribution and chosen functional.
  **Cost:** exponential category table in neighborhood size.
- **#bb.metric.info.interaction-information-distribution — Interaction-information distribution.** Signed higher-order
  information interactions among declared spatial variables.
  **Exactness:** ROI-exact from exact joint counts; otherwise Estimated.
  **Prerequisite:** variable set and interaction order.
  **Cost:** exponential in variable count.
- **#bb.metric.info.partial-information-decomposition — Partial-information decomposition.** Redundant, unique, and
  synergistic information supplied by declared source neighborhoods about a
  target bit/region.
  **Exactness:** Estimated/model-dependent because a PID definition must be
  chosen. **Prerequisite:** sources, target, PID measure, joint samples.
  **Cost:** high distribution-estimation/optimization cost.
- **#bb.metric.info.o-information — O-information.** Net redundancy-versus-synergy balance of a
  declared multivariate spatial distribution.
  **Exactness:** ROI-exact from exact joint probabilities; otherwise Estimated.
  **Prerequisite:** variable set and joint distribution.
  **Cost:** joint entropy computations, exponential table size.
- **#bb.metric.info.reference-distribution-divergence — Reference-distribution divergence.** KL divergence of observed
  world patterns from an explicitly named Bernoulli, Markov, lattice, grammar,
  or learned reference.
  **Exactness:** ROI-exact if both finite distributions are exact; usually
  Estimated/model-dependent. **Prerequisite:** reference support and smoothing
  for zero probabilities. **Cost:** linear in represented outcomes after fitting.
- **#bb.metric.info.symmetric-distribution-distance — Symmetric distribution distance.** Jensen-Shannon divergence (or
  its metric square root) between two declared region, component, scale, or
  time distributions.
  **Exactness:** Exact from exact finite probabilities, otherwise Estimated.
  **Prerequisite:** aligned outcome alphabet and weighting.
  **Cost:** `O(outcome_count)` after distributions.
- **#bb.metric.info.model-relative-surprisal-field — Model-relative surprisal field.** `-log2 P(observation)` globally
  or per local pattern under a declared probabilistic world model.
  **Exactness:** Estimated/model-relative. **Prerequisite:** normalized model and
  support policy. **Cost:** model evaluation over requested cells/patterns.
- **#bb.metric.info.local-entropy-field — Local-entropy field.** Distribution of entropy computed in equal
  spatial windows; heterogeneity summaries remain views.
  **Exactness:** ROI-exact for exhaustive windows, otherwise Estimated.
  **Prerequisite:** finite ROI, window shape, edge rule.
  **Cost:** `O(V * window_volume)` naive; integral counts accelerate Bernoulli
  entropy.
- **#bb.metric.info.spatial-run-length-entropy — Spatial run-length entropy.** Entropy of occupied and off run
  lengths along a declared set of lattice lines.
  **Exactness:** ROI-exact/exact for occupied-bounded line convention.
  **Prerequisite:** line directions and treatment of unbounded exterior off runs.
  **Cost:** same traversal as `#bb.metric.geom.column-gap-distribution`/`#bb.metric.geom.occupied-chord-distribution`.
- **#bb.metric.info.spatial-permutation-entropy — Spatial permutation entropy.** Ordinal-pattern entropy of a
  declared scalar spatial sequence, such as column heights or shell masses.
  **Exactness:** Exact for the finite sequence; interpretation is Estimated.
  **Prerequisite:** ordered scalar sequence, embedding order, tie rule.
  **Cost:** `O(sequence_length * embedding_order log embedding_order)`.
- **#bb.metric.info.categorical-attribute-entropy — Categorical attribute entropy.** Entropy/effective diversity of
  an already-defined categorical world distribution such as component sizes,
  degrees, motifs, normals, or chunk patterns. The source metric remains part of
  the view key rather than spawning one definition per attribute.
  **Exactness:** Exact from exact category counts. **Prerequisite:** source
  distribution. **Cost:** `O(category_count)`.
- **#bb.metric.info.general-purpose-description-length — General-purpose description length.** Bit length produced by a
  named deterministic compressor from a mathematically canonical finite
  occupancy sequence. This is an information upper bound, not a disk/storage
  metric.
  **Exactness:** Exact for that compressor and sequence; only an Estimated upper
  bound on algorithmic complexity. **Prerequisite:** finite bounding convention,
  canonical sequence, compressor/version. **Cost:** linear-ish in sequence size.
- **#bb.metric.info.lempel-ziv-phrase-complexity — Lempel-Ziv phrase complexity.** Number/novelty rate of phrases in
  a declared canonical occupancy sequence.
  **Exactness:** Exact for algorithm and sequence. **Prerequisite:** finite
  sequence and LZ variant. **Cost:** near `O(sequence_length)`.
- **#bb.metric.info.spatial-grammar-complexity — Spatial grammar complexity.** Rule count, description length, and
  derivation depth of a grammar reproducing the finite world.
  **Exactness:** Minimum grammar is generally intractable; Estimated by a named
  inference algorithm. **Prerequisite:** grammar formalism and search budget.
  **Cost:** algorithm-dependent, potentially exponential.
- **#bb.metric.info.octree-description-length — Octree description length.** Bits needed by a named finite-root
  octree code over occupancy; depth profile is a view.
  **Exactness:** Exact for chosen root, alignment, and code.
  **Prerequisite:** finite root cube and encoding grammar.
  **Cost:** roughly `O(N * depth)`.
- **#bb.metric.info.coordinate-gap-description-length — Coordinate-gap description length.** Bits required to encode
  lexicographically ordered coordinate differences using a named universal
  integer code.
  **Exactness:** Exact for ordering and code. **Prerequisite:** ordering, signed
  mapping, integer code. **Cost:** `O(N log N)` sort or `O(N)` from maintained
  order.
- **#bb.metric.info.enumerative-occupancy-lower-bound — Enumerative occupancy lower bound.** `log2 choose(V,N_R)` bits to
  select occupied cells inside a declared ROI, plus no implementation-specific
  overhead.
  **Exactness:** ROI-exact as a combinatorial quantity; logarithm displayed
  numerically. **Prerequisite:** finite `R` and `#bb.metric.world.roi-occupancy-measure`.
  **Cost:** `O(1)` high-precision special-function work from `V,N_R`.
- **#bb.metric.info.block-decomposition-algorithmic-estimate — Block-decomposition algorithmic estimate.** CTM/BDM-style sum of
  tabulated small-pattern complexities plus repetition terms.
  **Exactness:** Estimated and table/machine dependent.
  **Prerequisite:** decomposition, CTM table, overlap rule.
  **Cost:** `O(number_of_patches)` lookup after expensive external table creation.
- **#bb.metric.info.normalized-compression-distance — Normalized compression distance.** Compressor-based similarity
  `NCD(A,B)` between two finite worlds/components.
  **Exactness:** Estimated proxy for normalized information distance.
  **Prerequisite:** canonical finite sequences, compressor, concatenation rule.
  **Cost:** a constant number of `#bb.metric.info.general-purpose-description-length` compression passes.
- **#bb.metric.info.algorithmic-mutual-information-estimate — Algorithmic mutual-information estimate.**
  `C(A)+C(B)-C(A,B)` using the same declared compressor/description model.
  **Exactness:** Estimated upper-bound proxy.
  **Prerequisite:** aligned canonical encodings and joint construction.
  **Cost:** several description-length evaluations.
- **#bb.metric.info.minimum-description-model-split — Minimum-description model split.** Model-description bits and
  residual-data bits for a fitted world generator; total and split are one
  measure.
  **Exactness:** Estimated/model-class relative.
  **Prerequisite:** candidate model class, optimizer, validation policy.
  **Cost:** dominated by model search/fitting.
- **#bb.metric.info.logical-depth-estimate — Logical-depth estimate.** Computation needed to generate the
  world from a near-shortest found description.
  **Exactness:** Estimated and reference-machine/budget dependent; exact logical
  depth is not computable generally. **Prerequisite:** description candidates,
  significance allowance, machine model. **Cost:** potentially enormous search.
- **#bb.metric.info.kolmogorov-complexity-target — Kolmogorov-complexity target.** Length of the shortest program
  producing the finite world on a named universal machine.
  **Exactness:** Uncomputable exactly; only explicit upper bounds such as
  `#bb.metric.info.general-purpose-description-length` through `#bb.metric.info.block-decomposition-algorithmic-estimate` may be reported.
  **Prerequisite:** universal-machine convention. **Cost:** no terminating exact
  collector exists.
- **#bb.metric.info.past-future-predictive-information — Past-future predictive information.** Mutual information between
  declared past and future world-state windows.
  **Exactness:** History; Estimated from finite samples except tiny exhaustive
  systems. **Prerequisite:** retained history, window definitions, state features.
  **Cost:** distribution estimation over `T` windows, potentially exponential in
  feature size.
- **#bb.metric.info.active-information-storage — Active information storage.** Information a location/region's own
  past supplies about its next state.
  **Exactness:** History+Rule; Estimated from trajectories.
  **Prerequisite:** autonomous transitions, history embedding, estimator.
  **Cost:** `O(TV)` samples plus density estimation.
- **#bb.metric.info.directed-information-flow-measure — Directed information-flow measure.** Transfer entropy/directed
  information from one declared location/region history to another's next state.
  **Exactness:** History+Rule; Estimated except tiny exhaustive transition tables.
  **Prerequisite:** source/target, lags, conditioning set, estimator.
  **Cost:** `O(TV)` per tested relation plus high-dimensional estimation.
- **#bb.metric.info.dynamical-entropy-production — Dynamical entropy production.** Log-likelihood asymmetry between
  forward and time-reversed world trajectories.
  **Exactness:** History+Rule; Estimated/model-dependent.
  **Prerequisite:** stochastic/path probability model and reversal convention.
  **Cost:** trajectory-model fitting and evaluation.
- **#bb.metric.info.topological-dynamical-entropy — Topological dynamical entropy.** Exponential growth rate of
  distinguishable allowed world trajectories.
  **Exactness:** Rule; exact only for specially analyzable finite systems,
  otherwise Estimated.
  **Prerequisite:** autonomous rule, finite partition/distance resolution.
  **Cost:** state-space exponential in general.
- **#bb.metric.info.measure-theoretic-dynamical-entropy — Measure-theoretic dynamical entropy.** Kolmogorov-Sinai entropy
  rate under a declared invariant measure/partition.
  **Exactness:** Rule; generally Estimated.
  **Prerequisite:** autonomous dynamics, invariant measure, generating partition
  assumptions. **Cost:** long trajectories and convergence analysis.
- **#bb.metric.info.predictive-causal-state-complexity — Predictive causal-state complexity.** Entropy and population of
  equivalence classes of histories having the same future distribution.
  **Exactness:** History+Rule; Estimated by computational-mechanics inference.
  **Prerequisite:** retained trajectories, feature alphabet, inference/tolerance.
  **Cost:** model search that grows rapidly with history depth.

## Structural complexity and emergence

There is no definition-independent "emergence score." Each entry below names a
specific observable or theory. A future UI must expose that name and its
prerequisites rather than folding unlike quantities into one impressive number.

- **#bb.metric.emerge.entropy-disequilibrium-complexity — Entropy-disequilibrium complexity.** Product/family combining
  normalized entropy with distance from a declared equilibrium distribution;
  LMC and SDL parameterizations are named views.
  **Exactness:** ROI-exact from exact finite probabilities; otherwise Estimated.
  **Prerequisite:** entropy normalization, equilibrium, disequilibrium distance,
  and any exponents. **Cost:** `O(outcome_count)` after distributions.
- **#bb.metric.emerge.effective-structural-complexity — Effective structural complexity.** Description length assigned
  to inferred regularities rather than stochastic residuals.
  **Exactness:** Estimated/model-class relative.
  **Prerequisite:** `#bb.metric.info.minimum-description-model-split` model/residual split and adequacy criterion.
  **Cost:** dominated by model search.
- **#bb.metric.emerge.model-sophistication — Model sophistication.** Complexity of the simplest adequate
  generative model at a declared significance allowance, distinct from total
  data description length.
  **Exactness:** Uncomputable generally; Estimated by bounded model search.
  **Prerequisite:** model class, significance, universal/reference machine.
  **Cost:** potentially enormous.
- **#bb.metric.emerge.assembly-index — Assembly index.** Minimum number of recursive join operations
  required to construct a shape while allowing reuse of previously made
  subassemblies.
  **Exactness:** Exact only for exhaustively solved small instances; otherwise
  Estimated/bounded. **Prerequisite:** primitive, join operation, equivalence and
  reuse rules. **Cost:** combinatorial/exponential.
- **#bb.metric.emerge.reusable-subassembly-distribution — Reusable-subassembly distribution.** Size, multiplicity, and
  nesting of repeated connected substructures under a declared equivalence.
  **Exactness:** Exact for bounded motif size and exhaustive matching; otherwise
  Estimated. **Prerequisite:** adjacency, transform equivalence, size bound.
  **Cost:** subgraph/isometry matching; exponential without a size bound.
- **#bb.metric.emerge.structural-motif-diversity — Structural motif diversity.** Effective number of distinct
  local spatial/graph motifs at a declared scale.
  **Exactness:** Exact for a finite motif dictionary and exhaustive placements.
  **Prerequisite:** source census such as `#bb.metric.topo.local-graph-motif-census`, equivalence rule.
  **Cost:** motif-census cost plus `O(category_count)`.
- **#bb.metric.emerge.motif-surprise-profile — Motif surprise profile.** Deviation/information surprise of
  observed motif counts relative to a declared null ensemble.
  **Exactness:** Estimated unless the null expectation is analytically exact.
  **Prerequisite:** motif census, null model, multiple-test policy.
  **Cost:** exact census plus analytic or `O(M)` null worlds.
- **#bb.metric.emerge.spatial-structural-heterogeneity — Spatial structural heterogeneity.** Between-window variation in
  density, motif composition, or other declared macrostate feature as window
  scale changes.
  **Exactness:** ROI-exact for exhaustive windows; Estimated when sampled.
  **Prerequisite:** finite ROI, window family, feature vector.
  **Cost:** `O(KV * feature_cost)` naive.
- **#bb.metric.emerge.recursive-hierarchy-measure — Recursive hierarchy measure.** Depth and branching distribution
  of statistically meaningful nested modules/regions under a declared
  decomposition.
  **Exactness:** Estimated/algorithm-relative.
  **Prerequisite:** decomposition objective, stopping rule, significance test.
  **Cost:** repeated clustering/segmentation, algorithm-dependent.
- **#bb.metric.emerge.structural-nestedness — Structural nestedness.** Degree to which smaller region/motif
  incidence sets are contained in larger ones.
  **Exactness:** Exact for a fixed incidence matrix and named nestedness
  functional. **Prerequisite:** region-feature incidence construction.
  **Cost:** typically `O(rows * columns)` to `O(rows^2 * columns)`.
- **#bb.metric.emerge.crystalline-order-field — Crystalline order field.** Strength and orientation of repeated
  translations/lattice peaks relative to a declared crystal family.
  **Exactness:** ROI-exact for finite correlation/Fourier measures; model match is
  Estimated. **Prerequisite:** reference lattice or peak-selection rule.
  **Cost:** correlation or `O(V log V)` FFT.
- **#bb.metric.emerge.defect-distribution — Defect distribution.** Locations, classes, and density of
  mismatches from the best declared lattice, tiling, grammar, or repeated
  template.
  **Exactness:** Exact relative to a fixed aligned reference; Estimated when the
  reference/alignment is fitted. **Prerequisite:** structural reference and
  mismatch convention. **Cost:** fitting plus `O(V)` comparison.
- **#bb.metric.emerge.moran-spatial-autocorrelation — Moran spatial autocorrelation.** Moran's `I` for a declared
  scalar world field and spatial weight matrix.
  **Exactness:** Exact finite moments; null significance is Estimated.
  **Prerequisite:** scalar field, finite sites, weight matrix, centering rule.
  **Cost:** `O(site_count + weight_edges)`.
- **#bb.metric.emerge.geary-local-difference-autocorrelation — Geary local-difference autocorrelation.** Geary's `C`, retaining
  its distinct sensitivity to local differences.
  **Exactness:** Exact finite moments; null significance is Estimated.
  **Prerequisite:** same explicit field/weights as `#bb.metric.emerge.moran-spatial-autocorrelation`.
  **Cost:** `O(site_count + weight_edges)`.
- **#bb.metric.emerge.binary-join-count-deviation — Binary join-count deviation.** Like/unlike neighboring-state
  counts and their deviation from a declared random-label/random-field null.
  **Exactness:** ROI-exact counts; null inference Estimated/analytic by model.
  **Prerequisite:** finite ROI, adjacency, null ensemble.
  **Cost:** `O(V)` counts plus null calculation.
- **#bb.metric.emerge.bond-orientational-order — Bond-orientational order.** Spherical/cubic harmonic order of
  occupied-neighbor directions for declared degree `l`.
  **Exactness:** Exact sums with numerical harmonic evaluation.
  **Prerequisite:** neighbor definition, harmonic basis/order.
  **Cost:** `O(N * neighbor_count * l^2)`.
- **#bb.metric.emerge.regional-differentiation — Regional differentiation.** Diversity/divergence of macrostate
  feature distributions across a declared partition.
  **Exactness:** ROI-exact for fixed partition/features; Estimated if partition
  is learned. **Prerequisite:** regions, feature alphabet, divergence measure.
  **Cost:** feature extraction plus region histogram cost.
- **#bb.metric.emerge.regional-integration — Regional integration.** Dependence retained between declared
  regions relative to their individual uncertainties.
  **Exactness:** ROI-exact from an exact finite joint ensemble; usually Estimated.
  **Prerequisite:** regions, joint samples, dependence functional.
  **Cost:** high-dimensional joint-distribution estimation.
- **#bb.metric.emerge.effective-information — Effective information.** Information produced by applying a
  declared intervention distribution through the world transition map.
  **Exactness:** Rule; exact for an exhaustively enumerated finite transition
  system, otherwise Estimated. **Prerequisite:** autonomous rule, finite
  intervention set, macro/micro state map. **Cost:** state-space enumeration or
  `O(MT)` interventions.
- **#bb.metric.emerge.causal-emergence-gain — Causal-emergence gain.** Effective information at a declared
  macro description minus the correctly normalized micro description value.
  **Exactness:** Rule; Estimated except small finite systems.
  **Prerequisite:** `#bb.metric.emerge.effective-information` at both levels and a valid coarse-graining.
  **Cost:** intervention analysis at each candidate coarse-graining, often
  combinatorial.
- **#bb.metric.emerge.macro-predictive-advantage — Macro predictive advantage.** Out-of-sample predictive gain of
  a coarse world model over a micro model after explicit complexity penalty.
  **Exactness:** History+Rule; Estimated.
  **Prerequisite:** retained trajectories, candidate coarse maps, train/test
  split, scoring rule, complexity penalty. **Cost:** repeated model fitting.
- **#bb.metric.emerge.integrated-information — Integrated information.** Irreducible cause/effect information
  across the minimum partition under a named Phi definition.
  **Exactness:** Rule; exact only for tiny systems, definition-dependent.
  **Prerequisite:** autonomous finite transition model, intervention measure,
  Phi variant. **Cost:** generally exponential in system size.
- **#bb.metric.emerge.causal-density — Causal density.** Density/strength distribution of significant
  directed conditional influences among declared world regions.
  **Exactness:** History+Rule; Estimated.
  **Prerequisite:** region time series, causal model, conditioning/significance.
  **Cost:** many multivariate model fits.
- **#bb.metric.emerge.macro-to-micro-intervention-effect — Macro-to-micro intervention effect.** Residual influence of a
  macro intervention on micro futures after conditioning on available micro
  state; a concrete downward-causation assay.
  **Exactness:** Rule; Estimated unless transition system is exhaustively known.
  **Prerequisite:** valid interventions, micro/macro maps, confound control.
  **Cost:** `O(MT)` trials plus causal estimation.
- **#bb.metric.emerge.self-organization-rate — Self-organization rate.** Change in a declared internal order
  measure after subtracting order directly imposed through external writes.
  **Exactness:** History+Rule; Estimated/model-dependent.
  **Prerequisite:** autonomous rule, system boundary, external-write accounting,
  order measure. **Cost:** order measure over `T` plus attribution.
- **#bb.metric.emerge.perturbational-homeostasis — Perturbational homeostasis.** Recovery distribution of declared
  macrovariables after standardized world perturbations.
  **Exactness:** Rule; Estimated over trials.
  **Prerequisite:** baseline/target band, perturbation ensemble, recovery horizon.
  **Cost:** `O(MT * transition_cost)`.
- **#bb.metric.emerge.structural-novelty-rate — Structural novelty rate.** First appearances of previously
  unseen bounded motifs/equivalence classes per logical transition.
  **Exactness:** History; exact relative to finite detector, dictionary, and
  retained past. **Prerequisite:** motif detector/equivalence and history index.
  **Cost:** changed-neighborhood motif updates plus dictionary lookup.
- **#bb.metric.emerge.open-ended-complexity-growth-evidence — Open-ended complexity-growth evidence.** New maxima and
  saturation tests for a declared nontrivial complexity estimator as time and
  available space increase.
  **Exactness:** History+Rule; finite evidence only, never proof of unbounded
  growth. **Prerequisite:** autonomous run, estimator, resource/time schedule,
  saturation/null tests. **Cost:** repeated complexity measurement over `T`.
- **#bb.metric.emerge.morphological-diversity-through-time — Morphological diversity through time.** Effective number of
  distinct component/coherent-pattern shape classes observed in a history
  window.
  **Exactness:** History; exact relative to detector/equivalence, otherwise
  Estimated. **Prerequisite:** segmentation, alignment, equivalence threshold.
  **Cost:** component extraction and pair/indexed shape classification.
- **#bb.metric.emerge.coherent-structure-population — Coherent-structure population.** Count and state distribution of
  localized patterns satisfying a declared persistence/coherence detector.
  **Exactness:** History+Rule; exact relative to detector, not definition-free.
  **Prerequisite:** autonomous dynamics, detector, localization and identity
  rules. **Cost:** segmentation/tracking over `T`.
- **#bb.metric.emerge.causal-autonomy — Causal autonomy.** Fraction of a declared region's predictive
  information/causal influence attributable internally rather than across its
  boundary.
  **Exactness:** History+Rule; Estimated.
  **Prerequisite:** region boundary, causal model, conditioning history.
  **Cost:** multivariate causal estimation.
- **#bb.metric.emerge.environmental-adaptation — Environmental adaptation.** Retained improvement of a declared
  world pattern/region against a separately declared environment and objective
  after perturbation/change.
  **Exactness:** Rule; Estimated and undefined until environment/objective exist.
  **Prerequisite:** system/environment boundary, objective, exposure protocol,
  controls. **Cost:** repeated controlled trajectories.
- **#bb.metric.emerge.critical-model-evidence — Critical-model evidence.** Likelihood/evidence comparison among
  power-law/critical and named noncritical alternatives for scale-free event or
  structure distributions.
  **Exactness:** Estimated; an exponent alone is never this metric.
  **Prerequisite:** adequate samples, lower cutoff selection, alternatives,
  goodness-of-fit and held-out validation. **Cost:** model fitting plus bootstrap.
- **#bb.metric.emerge.rule-computational-sensitivity — Rule computational sensitivity.** Output sensitivity,
  canalization, and influence distribution of a future local bit rule under a
  declared neighborhood-configuration ensemble.
  **Exactness:** Rule; exact by full finite rule-table enumeration, otherwise
  Estimated. **Prerequisite:** local rule, neighborhood, configuration
  distribution. **Cost:** `O(rule_table_size * configuration_count)` exact.
- **#bb.metric.emerge.rule-quiescence-parameter — Rule quiescence parameter.** Fraction/distribution of local
  neighborhood configurations mapping away from a declared quiescent output
  (Langton-lambda family).
  **Exactness:** Rule; exact for a finite rule table.
  **Prerequisite:** quiescent state, local rule table, configuration weighting.
  **Cost:** `O(rule_table_size)`.

Computational universality and semantic meaning are deliberately not metric
definitions. Universality requires a construction/proof or a carefully bounded
capability result; no complexity curve establishes it. Semantic complexity is
undefined until an external grounding/task relation is supplied.

## Temporal world dynamics

Here `t -> t+1` means successive logical world states, not wall-clock time and
not scheduler behavior. Window length and sampling cadence are views recorded
with each result, not new metric definitions.

- **#bb.metric.dyn.occupancy-turnover-vector — Occupancy turnover vector.** Births
  `|S_(t+1) \ S_t|` and deaths `|S_t \ S_(t+1)|`; net growth, total activity,
  and normalized rates are views of this two-dimensional measure.
  **Exactness:** History; Exact from two states or a complete change set.
  **Prerequisite:** consecutive logical states. **Cost:** `O(P)` from changes or
  `O(N_t + N_(t+1))` set difference.
- **#bb.metric.dyn.occupied-bit-lifetime-distribution — Occupied-bit lifetime distribution.** Consecutive transition
  duration for which coordinates remain on; current ages, survival curve, and
  deletion hazard are views of completed/censored lifetimes.
  **Exactness:** History; Exact relative to retained origin time and censoring.
  **Prerequisite:** per-coordinate on/off event history.
  **Cost:** `O(P)` updates and `O(N)` current age state.
- **#bb.metric.dyn.first-activation-time-distribution — First-activation time distribution.** Earliest retained logical
  transition at which each coordinate became occupied.
  **Exactness:** History; Exact only from world inception or an explicitly
  left-censored start. **Prerequisite:** complete activation event history.
  **Cost:** `O(P)` updates/storage over ever-activated coordinates.
- **#bb.metric.dyn.revisit-distribution — Revisit distribution.** Number and spacing of separate on episodes
  at each coordinate.
  **Exactness:** History; Exact over retained history.
  **Prerequisite:** coordinate transition log. **Cost:** `O(P)` aggregation.
- **#bb.metric.dyn.occupancy-growth-trajectory — Occupancy growth trajectory.** `N_t` as a function of logical
  transition; growth rates and doubling/halving times are fitted/display views.
  **Exactness:** History; Exact counts, Estimated trend fits.
  **Prerequisite:** retained populations. **Cost:** `O(T)` storage/analysis.
- **#bb.metric.dyn.centroid-trajectory — Centroid trajectory.** World centroid position through logical
  state sequence; displacement, velocity, acceleration, and path length are
  views.
  **Exactness:** History; exact rational centroids, numeric derivatives.
  **Prerequisite:** `#bb.metric.geom.centroid` per state or maintained coordinate moments.
  **Cost:** `O(P)` incremental or `O(TN)` rescanning.
- **#bb.metric.dyn.second-moment-trajectory — Second-moment trajectory.** Evolution of covariance/principal-shape
  tensor; rotation, elongation, and spread changes are views.
  **Exactness:** History; exact moments, numerical eigen decomposition.
  **Prerequisite:** `#bb.metric.geom.second-moment-shape-tensor` over time. **Cost:** `O(P)` incremental moments.
- **#bb.metric.dyn.directional-front-trajectory — Directional-front trajectory.** Evolution of support/extreme
  coordinates in declared directions; advance and retreat speeds are views.
  **Exactness:** History; Exact for finite directions.
  **Prerequisite:** `#bb.metric.geom.directional-support-profile` per state. **Cost:** insertions cheap; deleting an
  extreme can require indexed search/rescan.
- **#bb.metric.dyn.radial-mass-growth-field — Radial mass-growth field.** Shell population as a function of
  logical time; front radius and radial growth exponent are views/fits.
  **Exactness:** History; exact shell counts, Estimated scaling fits.
  **Prerequisite:** `#bb.metric.world.origin-relative-radial-mass-distribution` per state. **Cost:** `O(P)` incremental shells.
- **#bb.metric.dyn.interface-growth-field — Interface-growth field.** Surface/height field through time;
  interface width, roughening, and growth exponents are views.
  **Exactness:** History; exact discrete fields, Estimated exponent fits.
  **Prerequisite:** surface convention and `#bb.metric.geom.height-field-roughness-distribution`-like field per state.
  **Cost:** surface updates or repeated `O(N)` extraction.
- **#bb.metric.dyn.component-lineage-graph — Component lineage graph.** Directed matching from components in
  one state to those in the next, representing births, deaths, merges, splits,
  and continuations.
  **Exactness:** History; Exact relative to overlap/identity convention.
  **Prerequisite:** `#bb.metric.topo.connected-component-size-distribution` per state and lineage rule.
  **Cost:** dynamic connectivity plus component-overlap matching.
- **#bb.metric.dyn.component-size-trajectories — Component-size trajectories.** Size histories along paths in the
  component lineage graph.
  **Exactness:** History; Exact relative to `#bb.metric.dyn.component-lineage-graph`.
  **Prerequisite:** component lineage. **Cost:** `O(number_of_lineage_edges)`.
- **#bb.metric.dyn.topology-event-sequence — Topology-event sequence.** Birth/death of components, tunnels, and
  cavities between consecutive world complexes.
  **Exactness:** History; Exact under a named zigzag/persistent-homology or
  representative matching convention. **Prerequisite:** `#bb.metric.topo.cubical-homology-measure` per state and
  cross-state feature matching. **Cost:** repeated homology; expensive.
- **#bb.metric.dyn.topological-state-transition-distribution — Topological-state transition distribution.** Transition counts
  between declared topological signatures such as `(beta_0,beta_1,beta_2)`.
  **Exactness:** History; Exact over retained sequence.
  **Prerequisite:** signature per state. **Cost:** `O(T)` after topology.
- **#bb.metric.dyn.barcode-change-distance — Barcode change distance.** Bottleneck or Wasserstein distance
  between mathematical persistent-homology barcodes of successive worlds.
  **Exactness:** History; Exact/numerical for finite barcodes.
  **Prerequisite:** `#bb.metric.topo.persistent-homology-barcode` per compared state and distance convention.
  **Cost:** barcode matching, commonly superlinear/cubic in interval count.
- **#bb.metric.dyn.state-space-trajectory-geometry — State-space trajectory geometry.** Cumulative symmetric-difference
  path length and endpoint distance; directness is their ratio view.
  **Exactness:** History; Exact. **Prerequisite:** consecutive state differences.
  **Cost:** `O(T)` after `#bb.metric.dyn.occupancy-turnover-vector`.
- **#bb.metric.dyn.coordinate-activity-field — Coordinate activity field.** Number of on/off transitions at each
  coordinate over retained history; hotspot concentration is a distribution
  view.
  **Exactness:** History; Exact over retained events.
  **Prerequisite:** complete coordinate change sets. **Cost:** `O(P_total)` time
  and up to one counter per changed coordinate.
- **#bb.metric.dyn.event-displacement-distribution — Event displacement distribution.** Spatial displacement between
  successive world-change events under a declared ordering for simultaneous
  changes.
  **Exactness:** History; Exact relative to event ordering; set-to-set variants
  may be Estimated. **Prerequisite:** spatial event log and simultaneity rule.
  **Cost:** `O(P_total)` for a total order.
- **#bb.metric.dyn.inter-event-interval-distribution — Inter-event interval distribution.** Logical-state gaps between
  world changes, globally or for a declared coordinate/region event stream.
  **Exactness:** History; Exact. **Prerequisite:** event stream and zero-event
  transition retention. **Cost:** `O(event_count)`.
- **#bb.metric.dyn.event-count-dispersion-across-scale — Event-count dispersion across scale.** Fano factor of event counts
  in equal logical-time windows as a function of window size.
  **Exactness:** History; exact sample moments, Estimated process property.
  **Prerequisite:** retained event counts and window family.
  **Cost:** `O(T * window_count)` naive.
- **#bb.metric.dyn.adjacent-window-event-variation — Adjacent-window event variation.** Allan-factor family measuring
  squared differences between neighboring window counts versus scale.
  **Exactness:** History; exact sample statistic, Estimated process property.
  **Prerequisite:** event-count sequence and window family.
  **Cost:** `O(T * window_count)` naive.
- **#bb.metric.dyn.spatiotemporal-event-correlation — Spatiotemporal event correlation.** Pair correlation of bit-change
  events as a function of spatial displacement and logical-time lag.
  **Exactness:** History; Exact if all event pairs are counted, otherwise
  Estimated. **Prerequisite:** finite spatial/time lag bins and edge correction.
  **Cost:** `O(P_total^2)` exact; sampled/FFT alternatives.
- **#bb.metric.dyn.temporal-autocorrelation-field — Temporal autocorrelation field.** Autocorrelation versus lag of a
  declared world observable or local occupancy field; decorrelation time is a
  view.
  **Exactness:** History; exact finite sample statistic, Estimated stationary
  process property. **Prerequisite:** evenly sampled logical-state series.
  **Cost:** `O(T^2)` naive or `O(T log T)` FFT per scalar series.
- **#bb.metric.dyn.lagged-world-distance — Lagged world distance.** Symmetric-difference or declared shape
  distance between `S_t` and `S_(t+lag)` as a function of lag.
  **Exactness:** History; Exact for enumerated pairs, otherwise Estimated.
  **Prerequisite:** retained states/change reconstruction and distance choice.
  **Cost:** potentially `O(T^2 N)`; sample or incremental methods.
- **#bb.metric.dyn.recurrence-structure — Recurrence structure.** Recurrence matrix under a declared state
  distance/threshold; recurrence rate, diagonal determinism, and laminarity are
  views.
  **Exactness:** History; Exact for all retained state pairs.
  **Prerequisite:** state distance and recurrence threshold.
  **Cost:** `O(T^2 * distance_cost)`.
- **#bb.metric.dyn.exact-state-cycle-measure — Exact state-cycle measure.** Earliest recurrence, preperiod, and
  period of exactly equal world states.
  **Exactness:** History+Rule; Exact when equality is confirmed and history starts
  from the initial condition. **Prerequisite:** autonomous deterministic rule and
  retained/exactly recoverable states. **Cost:** equality indexing plus state
  comparisons; state space may be unbounded.
- **#bb.metric.dyn.attractor-destination-distribution — Attractor destination distribution.** Fixed point/cycle reached
  from a declared ensemble of finite initial worlds; basin sizes and basin
  entropy are views.
  **Exactness:** Rule; exact only for exhaustively enumerated finite state spaces,
  otherwise Estimated. **Prerequisite:** autonomous rule, finite ROI/boundary,
  initial-state ensemble, cycle detector. **Cost:** exponential exhaustive or
  `O(MT)` sampled.
- **#bb.metric.dyn.transient-path-distribution — Transient-path distribution.** Length and state-space geometry of
  trajectories before an identified fixed point/cycle.
  **Exactness:** Rule; Exact for completed trajectories, Estimated over an
  ensemble. **Prerequisite:** `#bb.metric.dyn.exact-state-cycle-measure`/`#bb.metric.dyn.attractor-destination-distribution` attractor detection.
  **Cost:** total simulated transitions.
- **#bb.metric.dyn.temporal-frequency-spectrum — Temporal frequency spectrum.** Fourier/wavelet power of a declared
  scalar world observable over logical time; dominant period and spectral
  entropy are views.
  **Exactness:** History; exact discrete transform numerically, process
  interpretation Estimated. **Prerequisite:** sampled observable series and
  detrending/window convention. **Cost:** `O(T log T)` per series.
- **#bb.metric.dyn.empirical-influence-cone — Empirical influence cone.** Probability/effect of a standardized
  local perturbation at spatial offset `r` and logical lag `tau`.
  **Exactness:** Rule; Estimated over paired intervention trials.
  **Prerequisite:** autonomous rule, matched controls, perturbation and effect
  distance. **Cost:** `O(MT * world_transition_cost)`.
- **#bb.metric.dyn.perturbation-response-kernel — Perturbation response kernel.** Full average world-state
  difference trajectory caused by a declared intervention versus an identical
  matched control evolution.
  **Exactness:** Rule; exact per deterministic paired trial, Estimated over an
  ensemble. **Prerequisite:** controlled identical initial states and rule.
  **Cost:** `O(MT * world_transition_cost)` plus difference collection.
- **#bb.metric.dyn.transition-reversibility-relation — Transition reversibility relation.** Fraction/structure of finite
  state transitions whose reverse is also allowed under the same rule.
  **Exactness:** Rule; exact for an exhaustively known finite transition graph,
  otherwise Estimated. **Prerequisite:** autonomous rule and finite state domain
  or trajectory ensemble. **Cost:** state-space exponential or sampled.
- **#bb.metric.dyn.preimage-count-distribution — Preimage-count distribution.** Number of predecessor worlds
  mapping to each finite-domain successor; the no-preimage/Garden-of-Eden share
  is a view.
  **Exactness:** Rule; exact only under exhaustive finite-state enumeration.
  **Prerequisite:** autonomous rule, finite ROI and boundary rule.
  **Cost:** exponential in ROI volume.
- **#bb.metric.dyn.candidate-conservation-residual — Candidate-conservation residual.** Change per transition in a
  declared world quantity (population, parity, weighted coordinate moment, or
  other candidate invariant).
  **Exactness:** History+Rule; Exact for observed transitions, never by itself a
  universal conservation proof. **Prerequisite:** quantity definition and
  complete transition. **Cost:** `O(P)` if incrementally computable.
- **#bb.metric.dyn.rule-active-site-distribution — Rule-active-site distribution.** Sites/local neighborhoods where
  evaluating the autonomous rule could change the next world.
  **Exactness:** Rule; Exact for a deterministic finite-neighborhood rule.
  **Prerequisite:** rule and candidate-site/frontier definition.
  **Cost:** `O(candidate_sites * rule_cost)`.
- **#bb.metric.dyn.rule-transition-pattern-distribution — Rule-transition pattern distribution.** Counts of local
  neighborhood configurations producing each on/off/no-change transition.
  **Exactness:** History+Rule; Exact for evaluated sites/transitions.
  **Prerequisite:** local rule, neighborhood encoding, evaluated-site log.
  **Cost:** `O(evaluated_sites)`.
- **#bb.metric.dyn.causal-dependency-depth — Causal dependency depth.** Longest/complete distribution of
  rule-attributed chains from earlier changes to later changes.
  **Exactness:** History+Rule; Exact relative to retained dependency provenance,
  otherwise Estimated from interventions.
  **Prerequisite:** causal-parent convention or controlled perturbation data.
  **Cost:** provenance DAG construction/traversal.
- **#bb.metric.dyn.regional-phase-coherence — Regional phase coherence.** Phase/coherence distribution among
  declared regional world observables.
  **Exactness:** History+Rule; numerical sample statistic, process property
  Estimated. **Prerequisite:** regions, observables, phase/coherence estimator.
  **Cost:** signal transforms/cross-spectra over region pairs.
- **#bb.metric.dyn.regional-phase-lag-distribution — Regional phase-lag distribution.** Lag maximizing or phasing the
  dependence between oscillating regional signals.
  **Exactness:** History+Rule; Estimated from finite signals.
  **Prerequisite:** region series and lag/phase estimator.
  **Cost:** cross-correlation/FFT for each tested pair.
- **#bb.metric.dyn.traveling-wave-field — Traveling-wave field.** Wavelength, direction, and phase/group
  velocity inferred from spatiotemporal occupancy/order correlations.
  **Exactness:** History+Rule; Estimated.
  **Prerequisite:** autonomous spatial field history with sufficient duration.
  **Cost:** multidimensional FFT/correlation or fitted tracking.
- **#bb.metric.dyn.coherent-pattern-track-distribution — Coherent-pattern track distribution.** Lifetimes, displacements,
  velocities, accelerations, and shape drift along identities produced by the
  declared coherent-structure detector.
  **Exactness:** History+Rule; Exact relative to detector/matching, otherwise
  Estimated. **Prerequisite:** `#bb.metric.emerge.coherent-structure-population` and identity association rule.
  **Cost:** segmentation and assignment over `T`.
- **#bb.metric.dyn.pattern-mean-squared-displacement — Pattern mean-squared displacement.** Mean squared displacement
  versus lag for tracked coherent patterns; diffusion exponent is a fitted view.
  **Exactness:** History+Rule; exact sample curve, Estimated scaling law.
  **Prerequisite:** `#bb.metric.dyn.coherent-pattern-track-distribution` tracks. **Cost:** `O(track_samples * lag_count)`.
- **#bb.metric.dyn.pattern-interaction-outcome-distribution — Pattern-interaction outcome distribution.** Classified outcomes
  when multiple tracked coherent structures enter a declared interaction zone.
  **Exactness:** History+Rule; exact relative to detector/classifier, otherwise
  Estimated. **Prerequisite:** tracks, interaction-zone definition, outcome
  equivalence. **Cost:** event detection plus shape classification.
- **#bb.metric.dyn.replication-event-distribution — Replication event distribution.** Parent/copy events, production
  rate, generation interval, and geometric fidelity under a declared
  pattern-replication criterion.
  **Exactness:** History+Rule; exact relative to criterion, not definition-free.
  **Prerequisite:** pattern identities, ancestry rule, copy threshold.
  **Cost:** tracking plus pairwise/indexed shape comparison.
- **#bb.metric.dyn.hysteresis-response — Hysteresis response.** Difference/loop in a declared world order
  parameter under increasing versus decreasing autonomous-rule control
  parameter sweeps.
  **Exactness:** Rule; exact sample trajectories, Estimated equilibrium response.
  **Prerequisite:** parameterized rule, matched sweep protocol, equilibration
  policy. **Cost:** two or more long parameter sweeps.
- **#bb.metric.dyn.damage-spreading-curve — Damage-spreading curve.** Expected next/future symmetric-difference
  size as a function of initial perturbation size and horizon.
  **Exactness:** Rule; exact per paired deterministic trial, Estimated over
  perturbation ensemble. **Prerequisite:** matched initial worlds, perturbation
  ensemble, autonomous rule. **Cost:** `O(MT * world_transition_cost)`.
- **#bb.metric.dyn.derrida-response-map — Derrida response map.** Expected next-step world distance as a
  function of current world distance under a declared pair ensemble.
  **Exactness:** Rule; Estimated except exhaustively enumerated finite systems.
  **Prerequisite:** autonomous rule, pair ensemble, distance measure.
  **Cost:** `O(M * world_transition_cost)` per distance bin.
- **#bb.metric.dyn.avalanche-distribution — Avalanche distribution.** Size, duration, spatial area, and shape
  of causally connected bursts of world changes; branching ratio and exponent
  fits are views.
  **Exactness:** History+Rule; exact relative to event-linking rule, statistical
  laws Estimated. **Prerequisite:** autonomous event history, causal/space-time
  linking thresholds. **Cost:** event graph construction plus fitting.
- **#bb.metric.dyn.recovery-time-distribution — Recovery-time distribution.** Time for declared macrovariables to
  return to a baseline band after standardized perturbations.
  **Exactness:** Rule; exact per completed trial, Estimated over ensemble.
  **Prerequisite:** `#bb.metric.dyn.perturbation-response-kernel`, baseline band, censoring/horizon convention.
  **Cost:** intervention trajectory cost.
- **#bb.metric.dyn.issuance-provenance-episode-relation — Issuance-provenance episode relation.** For every actual mutable
  off-to-on transition, the tuple `(coordinate, mint-occurrence tag, optional
  consuming burn-occurrence tag, active|closed)` reconstructed in receipt
  order. Re-minting a burned coordinate creates a distinct episode. Supply,
  mint/burn totals, lifetimes, and revisit rates are views already owned by
  `#bb.metric.world.mutable-occupied-bit-population`, `#bb.metric.batch.attempt-count`,
  `#bb.metric.batch.resolved-edit-count`, `#bb.metric.batch.state-changing-edit-count`,
  `#bb.metric.batch.transition-direction-vector`, `#bb.metric.batch.state-no-op-vector`,
  `#bb.metric.dyn.occupancy-turnover-vector`, `#bb.metric.dyn.occupied-bit-lifetime-distribution`,
  and `#bb.metric.dyn.revisit-distribution`.
  **Exactness:** History-exact from a verified complete genesis prefix.
  **Prerequisite:** event-occurrence tags and transition-classified receipts.
  **Cost:** `O(E)` rebuild or maintained `O(1)` per actual state change.
- **#bb.metric.dyn.genesis-occupancy-provenance-relation — Genesis-occupancy provenance relation.** For every mutable
  coordinate already on in the declared initial world, the coordinate,
  genesis/import source tag, and authorization/provenance status. This is
  distinct from off-to-on issuance episodes, which begin only after genesis.
  **Exactness:** Exact with a complete genesis/import record.
  **Prerequisite:** canonical initial world and genesis/import provenance.
  **Cost:** `O(N_genesis)` once per genesis/import.

## Multiscale and finite-range fractal analysis

Raw curves are primary data in this section. A fitted slope, exponent, or
"dimension" is only a view over a declared finite interval and must obey the
second interpretation guard.

- **#bb.metric.scale.occupied-box-count-curve — Occupied box-count curve.** Number of grid boxes intersecting
  `S` as a function of box size and alignment.
  **Exactness:** Exact for enumerated finite sizes/offsets.
  **Prerequisite:** box sizes, lattice alignment, treatment of negative
  coordinates. **Cost:** `O(KN)` per alignment.
- **#bb.metric.scale.mass-radius-curve — Mass-radius curve.** Occupied population within radius `r` of a
  declared center ensemble as a function of `r`.
  **Exactness:** Exact for exhaustively evaluated centers/radii; Estimated for
  sampled centers. **Prerequisite:** center ensemble, norm, edge correction.
  **Cost:** `O(KN)` for one center; up to `O(KN^2)` all centers naive.
- **#bb.metric.scale.correlation-integral-curve — Correlation-integral curve.** Fraction/count of occupied pairs
  within distance `r` as a function of `r`.
  **Exactness:** Exact if all pairs are enumerated; otherwise Estimated.
  **Prerequisite:** norm, radius bins, finite-size/edge convention.
  **Cost:** `O(N^2)` exact; spatial index or pair sampling for scale.
- **#bb.metric.scale.generalized-dimension-spectrum — Generalized-dimension spectrum.** Rényi dimensions `D_q` inferred
  from scale-dependent box probabilities across declared `q`.
  **Exactness:** Estimated for finite scale ranges.
  **Prerequisite:** box probability measures, `q` range, scale range, handling of
  empty boxes. **Cost:** `O(KN * q_count)` plus regressions.
- **#bb.metric.scale.multifractal-singularity-spectrum — Multifractal singularity spectrum.** Estimated `f(alpha)` and
  local exponent weights from a named direct or Legendre method; spectrum width
  is a view.
  **Exactness:** Estimated. **Prerequisite:** `#bb.metric.scale.generalized-dimension-spectrum`-quality partition sums,
  method, stable `q`/scale ranges. **Cost:** generalized-dimension work plus
  transforms/bootstrap.
- **#bb.metric.scale.local-scaling-dimension-distribution — Local scaling-dimension distribution.** Per-location fitted
  mass-growth exponent across a declared neighborhood scale range.
  **Exactness:** Estimated. **Prerequisite:** sampled/all centers, radii, minimum
  support and edge correction. **Cost:** `O(KN * center_count)` naive.
- **#bb.metric.scale.lacunarity-curve — Lacunarity curve.** Normalized variance/higher moments of mass in
  gliding or disjoint windows versus window scale.
  **Exactness:** ROI-exact for exhaustive windows; Estimated when sampled.
  **Prerequisite:** finite ROI, window shape, placement/alignment, empty handling.
  **Cost:** `O(KV)` with integral volumes for simple windows.
- **#bb.metric.scale.local-porosity-distribution-across-scale — Local-porosity distribution across scale.** Distribution of off
  fraction in windows as window size changes.
  **Exactness:** ROI-exact for exhaustive windows; Estimated when sampled.
  **Prerequisite:** finite ROI, window family and boundary policy.
  **Cost:** `O(KV)` with integral counts.
- **#bb.metric.scale.dilation-volume-curve — Dilation-volume curve.** Occupied volume after morphological
  dilation by radius `r` (Minkowski sausage) versus `r`.
  **Exactness:** ROI-exact/exact while the finite explored exterior contains each
  dilation. **Prerequisite:** structuring metric/element and boundary padding.
  **Cost:** distance transform `O(V)` plus `O(K)` threshold counts, or repeated
  sparse dilation.
- **#bb.metric.scale.erosion-survival-curve — Erosion-survival curve.** Occupied volume remaining after erosion
  radius `r`.
  **Exactness:** Exact for a named structuring element.
  **Prerequisite:** solid-voxel morphology and erosion boundary convention.
  **Cost:** occupied distance transform plus threshold counts.
- **#bb.metric.scale.opening-granulometry — Opening granulometry.** Occupied mass retained/removed by
  morphological openings as structuring-element size and shape vary.
  **Exactness:** ROI-exact/exact for finite deterministic morphology.
  **Prerequisite:** structuring-element family and boundary rule.
  **Cost:** `O(KV)` naive; algorithm-dependent.
- **#bb.metric.scale.closing-granulometry — Closing granulometry.** Off-volume filled/remaining after
  morphological closings across structuring-element scales.
  **Exactness:** ROI-exact. **Prerequisite:** finite ROI/exterior convention and
  structuring-element family. **Cost:** `O(KV)` naive.
- **#bb.metric.scale.coarse-cell-occupancy-distribution — Coarse-cell occupancy distribution.** Distribution of occupied
  mass/fraction in partition cells versus coarse cell size.
  **Exactness:** Exact for finite enumerated grids; alignment is a parameter.
  **Prerequisite:** cell-size/alignment family and included finite extent.
  **Cost:** `O(KN)`.
- **#bb.metric.scale.density-fluctuation-curve — Density-fluctuation curve.** Variance/covariance of coarse-cell
  occupied mass versus scale; variance-to-mean/Taylor slopes are fit views.
  **Exactness:** Exact finite sample moments; scaling law Estimated.
  **Prerequisite:** `#bb.metric.scale.coarse-cell-occupancy-distribution`, weighting and finite-size correction.
  **Cost:** `O(total coarse cells across scales)`.
- **#bb.metric.scale.pair-correlation-field — Pair-correlation field.** Occupied-pair density relative to a
  declared homogeneous/reference process as a function of displacement or
  radius.
  **Exactness:** ROI-exact from all pairs with exact edge correction; otherwise
  Estimated. **Prerequisite:** finite observation window, null intensity, bins,
  edge correction. **Cost:** `O(N^2)` exact or FFT/sampling.
- **#bb.metric.scale.ripley-neighborhood-curve — Ripley neighborhood curve.** `K(r)`/transformed `L(r)` cumulative
  neighbor excess relative to a declared point-process reference.
  **Exactness:** ROI-exact for exhaustive pairs and exact edge correction;
  otherwise Estimated. **Prerequisite:** point representation, finite window,
  null and edge correction. **Cost:** pair-query cost across radii.
- **#bb.metric.scale.spatial-semivariogram — Spatial semivariogram.** Expected half squared difference of a
  declared occupancy/property field versus displacement.
  **Exactness:** ROI-exact for all valid pairs; otherwise Estimated.
  **Prerequisite:** scalar field, finite ROI, displacement bins/edge rule.
  **Cost:** `O(V^2)` naive, FFT or sampling alternatives.
- **#bb.metric.scale.structure-factor-field — Structure-factor field.** Fourier-domain density-fluctuation
  power `S(k)` over a declared finite window.
  **Exactness:** Exact discrete transform numerically; continuum/process
  interpretation Estimated. **Prerequisite:** finite rasterization/window,
  detrending and normalization. **Cost:** `O(V log V)` FFT.
- **#bb.metric.scale.spectral-anisotropy-field — Spectral anisotropy field.** Angular distribution/tensor of
  structure-factor power as a function of spatial frequency.
  **Exactness:** Exact finite transform/binning, orientation interpretation
  numerical. **Prerequisite:** `#bb.metric.scale.structure-factor-field`, angular basis/bins.
  **Cost:** `O(V)` after FFT.
- **#bb.metric.scale.wavelet-coefficient-distribution — Wavelet coefficient distribution.** Spatial/orientation/scale
  distribution of coefficients under a named 3D wavelet; energy and entropy by
  scale are views.
  **Exactness:** Exact discrete transform numerically.
  **Prerequisite:** finite ROI, wavelet family, padding/boundary rule.
  **Cost:** `O(V)` to `O(V log V)`.
- **#bb.metric.scale.hurst-long-range-dependence-exponent — Hurst long-range-dependence exponent.** Fitted exponent for a
  declared scalar spatial field/sequence using a named estimator.
  **Exactness:** Estimated. **Prerequisite:** field, estimator, scale interval,
  stationarity/detrending assumptions. **Cost:** estimator dependent, commonly
  `O(V log V)`.
- **#bb.metric.scale.surface-roughness-exponent — Surface roughness exponent.** Fitted growth of interface width
  with observation length for a declared surface field.
  **Exactness:** Estimated. **Prerequisite:** `#bb.metric.geom.height-field-roughness-distribution`, window scales, valid
  single-valued surface and fit range. **Cost:** `O(K * surface_area)`.
- **#bb.metric.scale.correlation-length-field — Correlation-length field.** Characteristic decay/crossover
  length of spatial correlations by direction.
  **Exactness:** Estimated/model-dependent. **Prerequisite:** correlation curve,
  decay model, fitting range and anisotropy policy.
  **Cost:** correlation computation plus nonlinear fit.
- **#bb.metric.scale.scaling-crossover-distribution — Scaling crossover distribution.** Statistically supported
  breakpoint scales where a raw multiscale curve changes regime.
  **Exactness:** Estimated. **Prerequisite:** raw curve, candidate segment models,
  minimum segment length, model-selection criterion.
  **Cost:** segmented regression/model search.
- **#bb.metric.scale.scale-transform-self-similarity-defect — Scale-transform self-similarity defect.** Minimum normalized
  mismatch between the world and a transformed copy at scale factor `s`, over a
  declared translation/rotation family.
  **Exactness:** Exact over finite enumerated transforms; otherwise Estimated.
  **Prerequisite:** finite comparison extent, resampling rule, transform family.
  **Cost:** `O(K * transform_count * N)` or raster equivalent.
- **#bb.metric.scale.discrete-scale-ratio-evidence — Discrete-scale ratio evidence.** Candidate multiplicative scale
  factors producing repeated low self-similarity defect or aligned structural
  features.
  **Exactness:** Estimated/model-selected. **Prerequisite:** `#bb.metric.scale.scale-transform-self-similarity-defect` or other
  raw feature curve, candidate ratios, significance/null test.
  **Cost:** curve search plus bootstrap/null simulations.
- **#bb.metric.scale.log-periodicity-measure — Log-periodicity measure.** Periodic signal strength in residuals
  as a function of log scale.
  **Exactness:** Estimated. **Prerequisite:** detrended multiscale curve,
  frequency-search/multiple-test policy. **Cost:** spectral fit and bootstrap.
- **#bb.metric.scale.coarse-grained-entropy-curve — Coarse-grained entropy curve.** Declared occupancy/patch entropy
  after each named coarse-graining scale.
  **Exactness:** ROI-exact for finite transforms and exhaustive counts; otherwise
  Estimated. **Prerequisite:** coarse-graining operator and `INFO` entropy
  definition. **Cost:** `O(KV)` plus entropy counts.
- **#bb.metric.scale.cross-scale-mutual-information-field — Cross-scale mutual-information field.** Information shared
  between fine world variables and their coarse representations, or between
  adjacent coarse levels.
  **Exactness:** ROI-exact from exact joint counts; otherwise Estimated.
  **Prerequisite:** aligned coarse maps, sampling ensemble, joint estimator.
  **Cost:** `O(KV)` collection plus joint distributions.
- **#bb.metric.scale.renormalization-trajectory — Renormalization trajectory.** Vector of declared normalized
  world observables after repeated application of a named coarse-graining
  operator; successive/fixed-point distances are views.
  **Exactness:** Exact for deterministic finite transforms, numerical for
  normalized observables. **Prerequisite:** operator, normalization, observable
  vector, finite boundary rule. **Cost:** repeated coarse transforms, `O(KV)`.
- **#bb.metric.scale.boundary-box-count-curve — Boundary box-count curve.** Number of boxes intersecting the
  occupied/exterior boundary versus box size.
  **Exactness:** Exact for enumerated sizes/alignments.
  **Prerequisite:** boundary extraction and box convention.
  **Cost:** `O(K * boundary_size)` per alignment.
- **#bb.metric.scale.spectral-dimension — Spectral dimension.** Fitted exponent of random-walk return
  probability versus walk length on `G_a`.
  **Exactness:** Estimated for finite graphs/ranges.
  **Prerequisite:** `#bb.metric.topo.random-walk-return-distribution`, walk-time interval, finite-size cutoff.
  **Cost:** random-walk/spectral work plus regression.
- **#bb.metric.scale.walk-dimension — Walk dimension.** Fitted relation between random-walk
  displacement and walk length.
  **Exactness:** Estimated. **Prerequisite:** embedded graph walk ensemble,
  displacement norm, fit interval. **Cost:** `O(M * walk_length)`.
- **#bb.metric.scale.chemical-dimension — Chemical dimension.** Fitted growth of occupied mass with graph
  geodesic radius.
  **Exactness:** Estimated over finite radii/centers.
  **Prerequisite:** adjacency, sampled/all centers, finite-size correction.
  **Cost:** BFS growth balls, `O(center_count * (N+E))`.
- **#bb.metric.scale.topological-scale-signature — Topological scale signature.** Cubical cell counts, Betti
  numbers, and Euler view as functions of dilation/erosion/height scale.
  **Exactness:** Exact for a finite filtration and coefficient field.
  **Prerequisite:** filtration and `#bb.metric.topo.cubical-complex-incidence-measure`/`#bb.metric.topo.cubical-homology-measure` conventions.
  **Cost:** persistent-homology/filtration cost.
- **#bb.metric.scale.persistence-landscape-measure — Persistence landscape measure.** Functional/vector
  representation of `#bb.metric.topo.persistent-homology-barcode` barcodes across scale; image, landscape norms,
  and persistence entropy are views.
  **Exactness:** Exact/numerical after finite barcode construction.
  **Prerequisite:** barcode and representation kernel/grid.
  **Cost:** `O(interval_count * representation_resolution)`.
- **#bb.metric.scale.connectivity-scale-curve — Connectivity-scale curve.** Component-size distribution as a
  function of deterministic dilation, erosion, or coarse-graining scale.
  **Exactness:** Exact for enumerated transforms.
  **Prerequisite:** transform family and adjacency convention.
  **Cost:** repeated connectivity, roughly `O(K(N+E))` after transforms.
- **#bb.metric.scale.skeleton-scale-curve — Skeleton-scale curve.** Skeleton size/branch distribution after
  smoothing/opening at each scale.
  **Exactness:** Exact relative to deterministic morphology and skeleton method.
  **Prerequisite:** `#bb.metric.topo.topology-preserving-skeleton`, smoothing/operator scale family.
  **Cost:** `O(KV)` or sparse method dependent.
- **#bb.metric.scale.percolation-over-scale-response — Percolation-over-scale response.** Spanning status/probability as
  deterministic coarse scale or stochastic occupation scale changes.
  **Exactness:** Exact for deterministic transform; Estimated for random
  ensembles. **Prerequisite:** finite ROI, spanning rule, transform/probability
  family. **Cost:** `O(K(N+E))` deterministic or multiplied by `M`.
- **#bb.metric.scale.directional-scaling-profile — Directional scaling profile.** Separate raw mass/count/correlation
  scaling curves and fitted exponents along declared orientations.
  **Exactness:** exact finite curves, Estimated slopes.
  **Prerequisite:** orientation sectors, base scaling measure, edge correction.
  **Cost:** base measure multiplied by direction count.
- **#bb.metric.scale.rank-size-distribution-across-scale — Rank-size distribution across scale.** Ranked component, cavity,
  or motif-cluster sizes as the observation scale changes.
  **Exactness:** Exact ranks for each finite transform; law/exponent Estimated.
  **Prerequisite:** source segmentation per scale and tie rule.
  **Cost:** source extraction plus `O(items log items)` per scale.
- **#bb.metric.scale.scale-dependent-symmetry — Scale-dependent symmetry.** Exact/approximate rigid-symmetry
  structure after each named coarse-graining level.
  **Exactness:** Exact for finite transforms/symmetry tests; Estimated where
  resampling/alignment is optimized. **Prerequisite:** `#bb.metric.geom.exact-rigid-symmetry-group`/`#bb.metric.geom.approximate-symmetry-defect` and
  coarse operator. **Cost:** `O(K * symmetry_candidate_count * N_k)`.
- **#bb.metric.scale.scaling-support-range — Scaling support range.** Longest contiguous scale interval over
  which a declared model passes residual/stability criteria; number of octaves is
  a view.
  **Exactness:** Estimated/model-dependent. **Prerequisite:** raw curve,
  thresholds, minimum samples per interval. **Cost:** interval/model search.
- **#bb.metric.scale.scaling-model-evidence — Scaling-model evidence.** Goodness-of-fit and predictive evidence
  comparing power law against named alternatives over the same observations.
  **Exactness:** Estimated. **Prerequisite:** raw curve/data, likelihoods,
  cutoff-selection and held-out/bootstrap policy.
  **Cost:** repeated fits and resampling.
- **#bb.metric.scale.scaling-estimator-uncertainty — Scaling-estimator uncertainty.** Sampling/numerical confidence
  distribution for a declared multiscale estimate.
  **Exactness:** Estimated. **Prerequisite:** estimator, resampling or analytic
  error model, dependence-aware sampling unit.
  **Cost:** base estimator multiplied by bootstrap/resample count.
- **#bb.metric.scale.finite-size-sensitivity — Finite-size sensitivity.** Change in a scaling estimate when
  smallest/largest scales, ROI extent, or edge corrections are systematically
  varied.
  **Exactness:** Estimated diagnostic. **Prerequisite:** declared perturbation
  grid and base estimator. **Cost:** repeated base fits.
- **#bb.metric.scale.grid-alignment-sensitivity — Grid-alignment sensitivity.** Distribution of box/lacunarity/
  coarse statistics over all or sampled spatial grid offsets.
  **Exactness:** Exact for exhaustive finite offsets, otherwise Estimated.
  **Prerequisite:** base grid metric, box size and offset set.
  **Cost:** base collection multiplied by offset count.

## Collection tiers

- **Live:** only maintained `O(1)` or bounded `O(N)` snapshot projections should
  update continuously: population, bounds/extents, chunk measures, low moments,
  local surface/contact counts, and recent turnover.
- **Snapshot/on demand:** all-pairs geometry, global graph centrality/spectra,
  exterior flood fills, homology, model search, compression estimates, Fourier
  analysis, and multiscale curves should operate on an immutable world snapshot
  in a cancellable analysis job.
- **History-gated:** a metric labeled History remains unavailable until its
  complete required event/state window exists; it must display left/right
  censoring and sampling gaps.
- **Rule-gated:** a metric labeled Rule remains explicitly unavailable until an
  autonomous transition rule and every named intervention/boundary convention
  exist. Human edits alone must not be silently treated as autonomous dynamics.
