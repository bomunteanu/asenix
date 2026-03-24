use asenix::metrics::diversity::{
    assign_to_centroids, cluster_entropy, compute_frontier_diversity, random_project,
};

// ─── Entropy boundary conditions ─────────────────────────────────────────────

/// All atoms assigned to a single cluster: entropy must be 0.
#[test]
fn test_entropy_all_one_cluster() {
    let k = 5;
    let assignments: Vec<usize> = vec![0; 40];
    let h = cluster_entropy(&assignments, k);
    assert_eq!(h, 0.0, "entropy must be 0 when all atoms are in one cluster");
}

/// k clusters with exactly equal atom counts: entropy must equal log2(k).
#[test]
fn test_entropy_uniform_distribution() {
    let k = 8;
    let atoms_per_cluster = 10;
    let assignments: Vec<usize> = (0..k).flat_map(|c| vec![c; atoms_per_cluster]).collect();
    let h = cluster_entropy(&assignments, k);
    let expected = (k as f64).log2();
    assert!(
        (h - expected).abs() < 1e-10,
        "entropy of uniform distribution over k={k} clusters should be log2(k)={expected:.6}, got {h:.6}"
    );
}

/// Two clusters 50/50 → entropy = 1.0 bit.
#[test]
fn test_entropy_two_equal_clusters() {
    let assignments: Vec<usize> = (0..20).map(|i| i % 2).collect();
    let h = cluster_entropy(&assignments, 2);
    assert!(
        (h - 1.0).abs() < 1e-10,
        "50/50 split over 2 clusters should give entropy=1.0, got {h:.6}"
    );
}

// ─── Random projection stability ─────────────────────────────────────────────

/// Same seed → identical projection matrix → identical projected coordinates.
#[test]
fn test_fixed_projection_is_deterministic() {
    let dim = 640;
    let n_components = 15;
    let seed = 42u64;

    let v: Vec<f32> = (0..dim).map(|i| (i as f32) * 0.001).collect();
    let embeddings = vec![v.clone(); 5];

    let projected_a = random_project(&embeddings, n_components, seed);
    let projected_b = random_project(&embeddings, n_components, seed);

    for (a, b) in projected_a.iter().zip(projected_b.iter()) {
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x, y, "same seed must produce identical projection");
        }
    }
}

/// Different seeds → different projection matrices (with overwhelming probability).
#[test]
fn test_different_seeds_give_different_projections() {
    let dim = 640;
    let n_components = 15;
    let v: Vec<f32> = (0..dim).map(|i| (i as f32) * 0.001 + 1.0).collect();
    let embeddings = vec![v; 3];

    let projected_42 = random_project(&embeddings, n_components, 42);
    let projected_99 = random_project(&embeddings, n_components, 99);

    let any_diff = projected_42[0]
        .iter()
        .zip(projected_99[0].iter())
        .any(|(a, b)| (a - b).abs() > 1e-6);
    assert!(any_diff, "different seeds must produce different projection matrices");
}

/// Project N atoms with seed S, then project M additional atoms with the same seed.
/// Atoms that were close in the original space must be close in projected space.
/// This verifies stability: new atoms project into the existing reference frame.
#[test]
fn test_projection_preserves_proximity() {
    let dim = 640;
    let n_components = 15;
    let seed = 42u64;

    // Two clusters: A-atoms all near (1,0,0,...), B-atoms all near (-1,0,...,0)
    let a_base: Vec<f32> = std::iter::once(1.0_f32)
        .chain(std::iter::repeat(0.0_f32).take(dim - 1))
        .collect();
    let b_base: Vec<f32> = std::iter::once(-1.0_f32)
        .chain(std::iter::repeat(0.0_f32).take(dim - 1))
        .collect();

    // Small perturbation helper
    let perturb = |base: &Vec<f32>, idx: usize| -> Vec<f32> {
        base.iter()
            .enumerate()
            .map(|(i, &x)| if i == idx % dim { x + 0.01 } else { x })
            .collect()
    };

    let a1 = perturb(&a_base, 1);
    let a2 = perturb(&a_base, 2);
    let b1 = perturb(&b_base, 3);
    let b2 = perturb(&b_base, 4);

    // Project all four together
    let projected = random_project(&[a1, a2, b1, b2], n_components, seed);

    // Distance between same-cluster pair must be < distance between cross-cluster pair
    let dist = |u: &[f32], v: &[f32]| -> f32 {
        u.iter().zip(v.iter()).map(|(a, b)| (a - b).powi(2)).sum::<f32>().sqrt()
    };

    let d_aa = dist(&projected[0], &projected[1]);
    let d_bb = dist(&projected[2], &projected[3]);
    let d_ab = dist(&projected[0], &projected[2]);

    assert!(
        d_aa < d_ab,
        "within-cluster A distance {d_aa:.4} should be < cross-cluster distance {d_ab:.4}"
    );
    assert!(
        d_bb < d_ab,
        "within-cluster B distance {d_bb:.4} should be < cross-cluster distance {d_ab:.4}"
    );
}

// ─── K-means cluster recovery ─────────────────────────────────────────────────

/// Generate k synthetic cluster centers well-separated in 640-dim space.
/// k-means should recover all k clusters and the resulting entropy should be
/// close to log2(k) (balanced clusters) and strictly > 0.
#[test]
fn test_kmeans_recovers_known_clusters() {
    let dim = 640;
    let k: usize = 3;
    let atoms_per_cluster = 25;
    let noise_scale = 0.05_f32;

    // k orthogonal unit vectors as cluster centers
    let centers: Vec<Vec<f32>> = (0..k)
        .map(|c| {
            let mut v = vec![0.0_f32; dim];
            v[c] = 1.0; // one-hot in the first k dimensions
            v
        })
        .collect();

    // Generate atoms with small noise around each center
    let mut embeddings: Vec<Vec<f32>> = Vec::new();
    let mut true_labels: Vec<usize> = Vec::new();

    for (c, center) in centers.iter().enumerate() {
        for atom_idx in 0..atoms_per_cluster {
            // deterministic noise via LCG
            let noise: Vec<f32> = center
                .iter()
                .enumerate()
                .map(|(i, &x)| {
                    let t = ((c * 1000 + atom_idx * 100 + i) as f32 * 0.618_033_9).fract();
                    x + noise_scale * (t * 2.0 - 1.0)
                })
                .collect();
            embeddings.push(noise);
            true_labels.push(c);
        }
    }

    let result = compute_frontier_diversity(&embeddings, k);

    assert_eq!(result.k, k);
    assert_eq!(result.atom_count, k * atoms_per_cluster);
    assert_eq!(result.cluster_sizes.len(), k);
    assert_eq!(result.cluster_sizes.iter().sum::<usize>(), k * atoms_per_cluster);

    // Entropy should be positive (k-means found >1 non-empty cluster)
    assert!(
        result.entropy > 0.0,
        "entropy should be > 0 for well-separated clusters, got {}",
        result.entropy
    );

    // Max entropy = log2(k)
    let max_h = (k as f64).log2();
    assert!(
        (result.max_entropy - max_h).abs() < 1e-10,
        "max_entropy should be log2({k})={max_h:.6}, got {}",
        result.max_entropy
    );

    // Normalized entropy in [0, 1]
    assert!(result.normalized_entropy >= 0.0 && result.normalized_entropy <= 1.0);

    // Entropy should be at least 50% of maximum for well-separated balanced clusters
    assert!(
        result.normalized_entropy >= 0.5,
        "normalized entropy {:.3} should be >= 0.5 for balanced clusters",
        result.normalized_entropy
    );
}

/// With k=1 the entropy is always 0 regardless of input.
#[test]
fn test_frontier_diversity_k1_is_zero_entropy() {
    let embeddings: Vec<Vec<f32>> = (0..10).map(|i| vec![i as f32; 640]).collect();
    let result = compute_frontier_diversity(&embeddings, 1);
    assert_eq!(result.entropy, 0.0);
    assert_eq!(result.max_entropy, 0.0);
    assert_eq!(result.normalized_entropy, 0.0);
}

/// Empty input returns a zeroed result without panic.
#[test]
fn test_frontier_diversity_empty_input() {
    let result = compute_frontier_diversity(&[], 8);
    assert_eq!(result.entropy, 0.0);
    assert_eq!(result.atom_count, 0);
}

// ─── assign_to_centroids ─────────────────────────────────────────────────────

/// Points should be assigned to their nearest centroid.
#[test]
fn test_assign_to_nearest_centroid() {
    // 2-dim for easy inspection
    let centroids = vec![vec![0.0_f32, 0.0], vec![10.0, 10.0]];
    let points = vec![
        vec![0.5_f32, 0.5],  // near centroid 0
        vec![9.0, 9.5],      // near centroid 1
        vec![0.1, 0.2],      // near centroid 0
        vec![10.1, 9.9],     // near centroid 1
    ];
    let assignments = assign_to_centroids(&points, &centroids);
    assert_eq!(assignments, vec![0, 1, 0, 1]);
}
