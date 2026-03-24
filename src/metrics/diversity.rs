//! Frontier diversity: embedding-space clustering pipeline.
//!
//! Pipeline: raw 640-dim embeddings → random projection (15-dim) → k-means → Shannon entropy.
//!
//! ## Why random projection instead of refitting UMAP each tick?
//!
//! UMAP (and any transductive manifold method) changes the coordinate frame when
//! refitted — the same atom can move between clusters between consecutive metric
//! snapshots even if its embedding is unchanged, introducing phantom variance in
//! the diversity signal. A **deterministically seeded Gaussian random projection**
//! (Johnson-Lindenstrauss) fixes the coordinate frame for all time: the same
//! seed produces the same 640×15 matrix, so every atom projects to the same
//! low-dimensional point regardless of when or with how many other atoms it is
//! processed. The JL lemma guarantees that for n points, O(log n / ε²) dimensions
//! preserve all pairwise distances within relative error ε; at n=5000, ε=0.1,
//! ~15 dims suffice. New atoms arriving mid-sweep are projected with the same
//! matrix — no "transform into fixed embedding" machinery needed.
//!
//! ## Why 15 dimensions?
//!
//! At d=640 the concentration of measure phenomenon makes all pairwise distances
//! approximately equal (ratio max/min → 1 as d → ∞). k-means inertia minimisation
//! is meaningless when no cluster centroid is distinguishably closer than any
//! other. Reducing to d=15 (≈ log₂(5000) × 2) restores discriminative distance
//! structure without losing the neighbourhood topology.
//!
//! ## k choice
//!
//! k is configurable (`frontier_diversity_k`, default 8). For the `llm_efficiency`
//! NeurIPS domain there are ~5 independent hyperparameter axes (optimizer, learning
//! rate, batch size, LoRA rank, training strategy), so 5–10 clusters are
//! semantically meaningful. A fixed k is required for temporal comparability:
//! dynamic k (elbow/silhouette) would produce k₁ at t=1h and k₂ at t=2h,
//! making the entropy values incomparable across the sweep. The paper's claim is
//! about the *shape* of the diversity trajectory, not the absolute entropy value.

use rand::prelude::*;
use rand::SeedableRng;
use serde::Serialize;
use tracing::warn;

// ─── Fixed seeds ─────────────────────────────────────────────────────────────

/// Seed for the random projection matrix. Fixed forever — never change this.
/// Changing the seed would shift every atom's projected coordinates and break
/// temporal comparability of all historical metric snapshots.
const PROJECTION_SEED: u64 = 0xA5ED_1E5D_1E51_7100;

/// Base seed for k-means++ initialization. Each restart increments by 1.
const KMEANS_BASE_SEED: u64 = 0xA5EC_1A55_E751_0000;

/// Dimensionality of the projected space.
const N_PROJECTED: usize = 15;

/// Number of k-means restarts. Keep lowest-inertia result.
const KMEANS_RESTARTS: usize = 3;

/// Maximum Lloyd's iterations per restart.
const KMEANS_MAX_ITER: usize = 100;

// ─── Output type ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct FrontierDiversityData {
    /// Shannon entropy H = -Σ pᵢ log₂ pᵢ over the cluster size distribution.
    pub entropy: f64,
    /// Maximum possible entropy = log₂(k). Zero when k=1.
    pub max_entropy: f64,
    /// entropy / max_entropy ∈ [0, 1]. Zero when k=1 or all atoms in one cluster.
    pub normalized_entropy: f64,
    /// Number of atoms assigned to each cluster (length = k).
    pub cluster_sizes: Vec<usize>,
    /// Number of clusters used.
    pub k: usize,
    /// Number of atoms that were clustered (excludes atoms with no embedding).
    pub atom_count: usize,
}

impl Default for FrontierDiversityData {
    fn default() -> Self {
        Self {
            entropy: 0.0,
            max_entropy: 0.0,
            normalized_entropy: 0.0,
            cluster_sizes: vec![],
            k: 0,
            atom_count: 0,
        }
    }
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Top-level entry point.  `embeddings` is a slice of 640-dim vectors (f32).
/// Returns zeroed data if `embeddings` is empty or `k == 0`.
pub fn compute_frontier_diversity(embeddings: &[Vec<f32>], k: usize) -> FrontierDiversityData {
    if embeddings.is_empty() || k == 0 {
        return FrontierDiversityData {
            k,
            ..Default::default()
        };
    }

    // k=1: trivial — all atoms in one cluster, entropy=0.
    if k == 1 {
        return FrontierDiversityData {
            entropy: 0.0,
            max_entropy: 0.0,
            normalized_entropy: 0.0,
            cluster_sizes: vec![embeddings.len()],
            k: 1,
            atom_count: embeddings.len(),
        };
    }

    let projected = random_project(embeddings, N_PROJECTED, PROJECTION_SEED);
    let (assignments, _centroids) =
        kmeans_plus_plus(&projected, k, KMEANS_BASE_SEED, KMEANS_MAX_ITER, KMEANS_RESTARTS);

    let entropy = cluster_entropy(&assignments, k);
    let max_entropy = (k as f64).log2();
    let normalized_entropy = if max_entropy > 0.0 { entropy / max_entropy } else { 0.0 };

    let mut cluster_sizes = vec![0usize; k];
    for &a in &assignments {
        cluster_sizes[a] += 1;
    }

    FrontierDiversityData {
        entropy,
        max_entropy,
        normalized_entropy,
        cluster_sizes,
        k,
        atom_count: embeddings.len(),
    }
}

// ─── Random projection ───────────────────────────────────────────────────────

/// Gaussian random projection: R^d → R^n_components.
///
/// Each entry of the projection matrix is drawn from N(0, 1/n_components).
/// The seed is fixed so the projection is stable across calls — see module doc.
pub fn random_project(
    embeddings: &[Vec<f32>],
    n_components: usize,
    seed: u64,
) -> Vec<Vec<f32>> {
    if embeddings.is_empty() {
        return vec![];
    }
    let dim = embeddings[0].len();
    if dim == 0 || n_components == 0 {
        return vec![vec![]; embeddings.len()];
    }

    // Build the projection matrix: rows = n_components, cols = dim.
    // Entry ~ N(0, 1/n_components) for distance-preserving projection.
    let mut rng = rand::rngs::SmallRng::seed_from_u64(seed);
    let scale = (1.0_f32 / n_components as f32).sqrt();
    let matrix: Vec<Vec<f32>> = (0..n_components)
        .map(|_| {
            (0..dim)
                .map(|_| {
                    // Box-Muller from two uniform samples
                    let u1: f32 = rng.random::<f32>().max(1e-10);
                    let u2: f32 = rng.random::<f32>();
                    let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f32::consts::PI * u2).cos();
                    z * scale
                })
                .collect()
        })
        .collect();

    embeddings
        .iter()
        .map(|v| {
            let input = if v.len() == dim {
                v.as_slice()
            } else {
                warn!("embedding length {} != expected {}, zero-padding", v.len(), dim);
                v.as_slice()
            };
            (0..n_components)
                .map(|j| {
                    input
                        .iter()
                        .zip(matrix[j].iter())
                        .map(|(&x, &m)| x * m)
                        .sum::<f32>()
                })
                .collect()
        })
        .collect()
}

// ─── K-means ─────────────────────────────────────────────────────────────────

/// K-means++ with multiple restarts. Returns (assignments, centroids).
///
/// Runs `n_restarts` independent k-means++ runs and returns the result with the
/// lowest total inertia (sum of squared distances to nearest centroid). This
/// mitigates sensitivity to the random centroid initialisation.
pub fn kmeans_plus_plus(
    points: &[Vec<f32>],
    k: usize,
    base_seed: u64,
    max_iter: usize,
    n_restarts: usize,
) -> (Vec<usize>, Vec<Vec<f32>>) {
    if points.is_empty() || k == 0 {
        return (vec![], vec![]);
    }
    let k = k.min(points.len());

    let mut best_assignments = vec![0usize; points.len()];
    let mut best_centroids: Vec<Vec<f32>> = vec![];
    let mut best_inertia = f64::INFINITY;

    for restart in 0..n_restarts {
        let seed = base_seed.wrapping_add(restart as u64);
        let centroids = kmeanspp_init(points, k, seed);
        let (assignments, centroids, inertia) = lloyd(points, centroids, max_iter);
        if inertia < best_inertia {
            best_inertia = inertia;
            best_assignments = assignments;
            best_centroids = centroids;
        }
    }

    (best_assignments, best_centroids)
}

/// Assign each point in `projected` to its nearest centroid (Euclidean).
pub fn assign_to_centroids(points: &[Vec<f32>], centroids: &[Vec<f32>]) -> Vec<usize> {
    points
        .iter()
        .map(|p| nearest_centroid(p, centroids))
        .collect()
}

// ─── Shannon entropy ─────────────────────────────────────────────────────────

/// Shannon entropy H = -Σ pᵢ log₂ pᵢ over the cluster size distribution.
/// Returns 0.0 for empty input or all atoms in one cluster.
pub fn cluster_entropy(assignments: &[usize], k: usize) -> f64 {
    if assignments.is_empty() || k == 0 {
        return 0.0;
    }
    let mut counts = vec![0usize; k];
    for &a in assignments {
        if a < k {
            counts[a] += 1;
        }
    }
    let total = counts.iter().sum::<usize>() as f64;
    if total == 0.0 {
        return 0.0;
    }
    counts
        .iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = c as f64 / total;
            -p * p.log2()
        })
        .sum()
}

// ─── Private helpers ──────────────────────────────────────────────────────────

/// K-means++ centroid initialization.
/// First centroid: uniform random. Each subsequent centroid: sampled with
/// probability proportional to D(x)² (squared distance to nearest chosen centroid).
fn kmeanspp_init(points: &[Vec<f32>], k: usize, seed: u64) -> Vec<Vec<f32>> {
    let mut rng = rand::rngs::SmallRng::seed_from_u64(seed);
    let mut centroids: Vec<Vec<f32>> = Vec::with_capacity(k);

    // Pick first centroid uniformly
    let first = rng.random_range(0..points.len());
    centroids.push(points[first].clone());

    for _ in 1..k {
        // Compute D(x)² for each point
        let weights: Vec<f64> = points
            .iter()
            .map(|p| {
                let d = min_sq_dist(p, &centroids);
                d as f64
            })
            .collect();

        let total: f64 = weights.iter().sum();
        if total == 0.0 {
            // All points are exactly on existing centroids; pick randomly
            centroids.push(points[rng.random_range(0..points.len())].clone());
            continue;
        }

        // Weighted sampling
        let threshold = rng.random::<f64>() * total;
        let mut cumsum = 0.0;
        let mut chosen = points.len() - 1;
        for (i, &w) in weights.iter().enumerate() {
            cumsum += w;
            if cumsum >= threshold {
                chosen = i;
                break;
            }
        }
        centroids.push(points[chosen].clone());
    }

    centroids
}

/// One run of Lloyd's algorithm. Returns (assignments, centroids, inertia).
fn lloyd(
    points: &[Vec<f32>],
    mut centroids: Vec<Vec<f32>>,
    max_iter: usize,
) -> (Vec<usize>, Vec<Vec<f32>>, f64) {
    let k = centroids.len();
    let dim = points[0].len();
    let mut assignments = vec![0usize; points.len()];

    for _ in 0..max_iter {
        // Assignment step
        let new_assignments: Vec<usize> = points
            .iter()
            .map(|p| nearest_centroid(p, &centroids))
            .collect();

        let converged = new_assignments == assignments;
        assignments = new_assignments;

        // Update step: recompute centroids as mean of assigned points
        let mut sums = vec![vec![0.0_f64; dim]; k];
        let mut counts = vec![0usize; k];
        for (p, &a) in points.iter().zip(assignments.iter()) {
            for (s, &x) in sums[a].iter_mut().zip(p.iter()) {
                *s += x as f64;
            }
            counts[a] += 1;
        }
        for c in 0..k {
            if counts[c] > 0 {
                centroids[c] = sums[c].iter().map(|&s| (s / counts[c] as f64) as f32).collect();
            }
            // Empty cluster: centroid stays where it is (k-means++ init makes this rare)
        }

        if converged {
            break;
        }
    }

    let inertia: f64 = points
        .iter()
        .zip(assignments.iter())
        .map(|(p, &a)| sq_dist(p, &centroids[a]) as f64)
        .sum();

    (assignments, centroids, inertia)
}

/// Index of the nearest centroid to `point` by squared Euclidean distance.
fn nearest_centroid(point: &[f32], centroids: &[Vec<f32>]) -> usize {
    centroids
        .iter()
        .enumerate()
        .map(|(i, c)| (i, sq_dist(point, c)))
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Squared Euclidean distance between two equal-length slices.
fn sq_dist(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(&x, &y)| (x - y).powi(2))
        .sum()
}

/// Minimum squared distance from `point` to any centroid in `centroids`.
fn min_sq_dist(point: &[f32], centroids: &[Vec<f32>]) -> f32 {
    centroids
        .iter()
        .map(|c| sq_dist(point, c))
        .fold(f32::INFINITY, f32::min)
}
