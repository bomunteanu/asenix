# Research Agent Instructions — MNIST CNN Search

You are an autonomous ML research agent. Your goal: maximise MNIST val_accuracy by
modifying `train.py` and publishing results as atoms to the coordination hub.
Each run takes 1–3 minutes. You can complete several iterations per session.

---

## Setup

Read your credentials from the first line of your prompt — they look like:
```
AGENT_ID=<id>  API_TOKEN=<token>  PROJECT_ID=<project_id>
```
All MCP calls need `agent_id` + `api_token`. All atoms need `project_id`.

The MCP server `asenix` is already configured — do not call `register_agent_simple`.

---

## What you can edit in train.py

Open `train.py` and read it. The **AGENT-EDITABLE SECTION** at the top exposes:

| Parameter | Default | What it does |
|---|---|---|
| `LEARNING_RATE` | 0.001 | Optimiser step size |
| `BATCH_SIZE` | 128 | Mini-batch size |
| `OPTIMIZER` | "adam" | "adam" / "sgd" / "adamw" |
| `WEIGHT_DECAY` | 1e-4 | L2 regularisation |
| `MOMENTUM` | 0.9 | SGD only |
| `CONV_FILTERS` | [32, 64] | Filters in conv1, conv2 |
| `HIDDEN_SIZE` | 128 | FC hidden layer width |
| `DROPOUT` | 0.25 | Dropout rate |
| `ACTIVATION` | "relu" | "relu" / "gelu" / "silu" |
| `SCHEDULER` | "none" | "none" / "cosine" / "onecycle" |
| `NOTES` | ... | One-line description of your change |

You may also edit the `Model` class — add conv layers, residual connections, BatchNorm, etc.
The only fixed contract: input `(B, 1, 28, 28)`, output `(B, 10)`.

**Do not touch** anything below `# DO NOT EDIT BELOW THIS LINE`.

---

## Research loop

### Step 0 — Orient

```json
{ "domain": "mnist_cnn" }
```
Call `get_field_map` to read any synthesis atoms summarising the current state.
Skip if the domain is brand new (no syntheses yet).

### Step 1 — Read recent results

```json
{
  "agent_id": "<AGENT_ID>",
  "api_token": "<API_TOKEN>",
  "domain": "mnist_cnn",
  "limit": 10
}
```
Call `search_atoms`, then `get_suggestions` with the same body.

Read each atom's `conditions` and `metrics`. Note:
- `ph_attraction` — how promising this direction has been
- `ph_novelty` — how unexplored it is
- `lifecycle: "contested"` — contradicting evidence exists here

### Step 2 — Choose a direction and state a hypothesis

Pick a direction (extend a successful config, or explore a novel one). Say out loud:

> "Atom X achieved val_acc=0.992 with [32,64] filters and Adam.
>  I'll try wider filters [64,128] because the model may be capacity-limited.
>  Hypothesis: wider filters will push val_acc above 0.994."

### Step 3 — Register your claim

```json
{
  "agent_id": "<AGENT_ID>",
  "api_token": "<API_TOKEN>",
  "hypothesis": "Wider filters [64,128] will outperform [32,64] baseline",
  "domain": "mnist_cnn",
  "conditions": {
    "optimizer": "adam",
    "conv_filters": "[64, 128]"
  }
}
```
Save the returned `atom_id` as your `claim_atom_id`.

### Step 4 — Edit and dry-run

Modify `train.py`. Set `NOTES` to a one-liner. Then:

```bash
python train.py --dry-run
```

Fix any errors. Then train for real:

```bash
python train.py
```

Each run takes 1–3 minutes. Watch the epoch log:
- val_acc should climb each epoch
- Loss stuck or NaN → learning rate likely too high
- val_acc ≈ 0.10 → model is broken (all same class)

At the end you'll see:
```
RESULT_JSON: {"val_accuracy": 0.9934, "val_loss": 0.0218, ...}
```

### Step 5 — Publish

Determine atom type: `val_accuracy >= 0.990` → `"finding"`, else `"negative_result"`.

```json
{
  "agent_id": "<AGENT_ID>",
  "api_token": "<API_TOKEN>",
  "atoms": [
    {
      "atom_type": "finding",
      "domain": "mnist_cnn",
      "project_id": "<PROJECT_ID>",
      "statement": "CNN with CONV_FILTERS=[64,128], HIDDEN_SIZE=128, Adam lr=0.001 achieves val_acc=0.9934 (baseline [32,64] was 0.9912) — wider filters add capacity without overfitting",
      "conditions": {
        "learning_rate": 0.001,
        "batch_size": 128,
        "optimizer": "adam",
        "weight_decay": 0.0001,
        "conv_filters": "[64, 128]",
        "hidden_size": 128,
        "dropout": 0.25,
        "activation": "relu",
        "scheduler": "none"
      },
      "metrics": [
        {"name": "val_accuracy",  "value": 0.9934, "unit": null, "direction": "maximize"},
        {"name": "val_loss",      "value": 0.0218, "unit": null, "direction": "minimize"},
        {"name": "train_time_s",  "value": 87.4,   "unit": "seconds", "direction": "minimize"},
        {"name": "total_params",  "value": 421130, "unit": "count",   "direction": "minimize"}
      ],
      "provenance": {
        "parent_ids": ["<claim_atom_id>"],
        "method_description": "Single training run, 5 epochs, Apple M4 Pro MPS",
        "environment": {"runtime": "pytorch", "hardware": "Apple M4 Pro"}
      }
    }
  ]
}
```

Replace all values with actuals from `RESULT_JSON`.
`conditions` must exactly reflect what is in `train.py`.
`conv_filters` must be serialised as a string: `"[32, 64]"`.

### Step 6 — Repeat from Step 1

You have time for multiple iterations. Each should have a clearer hypothesis than the last.

---

## Reminders

- `project_id` is mandatory in every atom — it comes from your prompt header
- `conditions` must exactly match `train.py` — this enables contradiction detection
- `conv_filters` is a string: `"[32, 64]"`, not a list
- `parent_ids` is a list of strings
- All metric `value` fields are floats
- Baseline val_accuracy with defaults ≈ 0.991. Target: > 0.995.

## Debugging

| Symptom | Fix |
|---|---|
| `dry-run` fails | Read the Python error, fix the editable section |
| MCP auth error | Check AGENT_ID and API_TOKEN from your prompt header |
| `torchvision` not found | `pip install torchvision` |
| val_acc stuck at 0.10 | Architecture is broken — check Model output shape |
| `get_suggestions` empty | Publish the baseline run first |
