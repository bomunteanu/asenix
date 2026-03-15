#!/usr/bin/env python3
"""
train.py — image classification training script for the Asenix agent loop.

Agents should edit the AGENT-EDITABLE SECTION below (hyperparameters + model
architecture). Do NOT modify NUM_EPOCHS or anything below the DO NOT EDIT marker.

Usage:
    python train.py               # run training
    python train.py --debug       # per-batch loss
    python train.py --dry-run     # validate edits compile, then exit

Requirements:
    pip install torch torchvision
"""

# ════════════════════════════════════════════════════════════════════════════
# AGENT-EDITABLE SECTION
# Modify anything in this block. Keep valid Python. Model must output 10
# logits for a (N, 3, 32, 32) input tensor.
# ════════════════════════════════════════════════════════════════════════════

# ── Hyperparameters ──────────────────────────────────────────────────────────
LEARNING_RATE    = 0.1
BATCH_SIZE       = 128
OPTIMIZER        = "sgd"        # "sgd" | "adam" | "adamw"
WEIGHT_DECAY     = 5e-4
MOMENTUM         = 0.9          # used by SGD only
SCHEDULER        = "onecycle"   # "cosine" | "step" | "onecycle" | "none"
AUGMENTATION     = "strong"     # "none" | "standard" | "strong"
LABEL_SMOOTHING  = 0.1          # 0.0 = standard cross-entropy
DROPOUT          = 0.1
NOTES            = "SE-ResNet ch=96 [3,3,3] strong aug + SGD+OneCycleLR + LS=0.1 — ch=80 iter1 achieved 0.9361 new best with 1.87% train/val gap; width scaling monotonically positive, ch=96 next unexplored step"

# ── Model definition ─────────────────────────────────────────────────────────
import torch.nn as nn
import torch.nn.functional as F


class SEBlock(nn.Module):
    """Squeeze-and-Excitation channel attention."""
    def __init__(self, channels, reduction=16):
        super().__init__()
        mid = max(channels // reduction, 4)
        self.fc = nn.Sequential(
            nn.Linear(channels, mid, bias=False),
            nn.ReLU(inplace=True),
            nn.Linear(mid, channels, bias=False),
            nn.Sigmoid(),
        )

    def forward(self, x):
        s = x.mean(dim=(2, 3))          # global average pool: (N, C)
        s = self.fc(s).unsqueeze(-1).unsqueeze(-1)   # (N, C, 1, 1)
        return x * s


class ResBlock(nn.Module):
    def __init__(self, in_ch, out_ch, stride=1):
        super().__init__()
        self.conv1 = nn.Conv2d(in_ch, out_ch, 3, stride=stride, padding=1, bias=False)
        self.bn1   = nn.BatchNorm2d(out_ch)
        self.conv2 = nn.Conv2d(out_ch, out_ch, 3, 1, padding=1, bias=False)
        self.bn2   = nn.BatchNorm2d(out_ch)
        self.se    = SEBlock(out_ch)
        self.skip  = nn.Sequential()
        if stride != 1 or in_ch != out_ch:
            self.skip = nn.Sequential(
                nn.Conv2d(in_ch, out_ch, 1, stride=stride, bias=False),
                nn.BatchNorm2d(out_ch),
            )

    def forward(self, x):
        out = F.relu(self.bn1(self.conv1(x)))
        out = self.bn2(self.conv2(out))
        out = self.se(out)
        return F.relu(out + self.skip(x))


class Model(nn.Module):
    """3-stage SE-ResNet. SE channel attention improves generalization with minimal param overhead."""

    NUM_BLOCKS   = [3, 3, 3]   # blocks per stage
    BASE_CHANNELS = 96          # channels in stage 1; stages 2/3 are ×2 and ×4

    def __init__(self):
        super().__init__()
        B = self.BASE_CHANNELS
        self.stem = nn.Sequential(
            nn.Conv2d(3, B, 3, 1, 1, bias=False),
            nn.BatchNorm2d(B),
            nn.ReLU(inplace=True),
        )
        self.s1 = self._stage(B,    B,    self.NUM_BLOCKS[0], stride=1)
        self.s2 = self._stage(B,    B*2,  self.NUM_BLOCKS[1], stride=2)
        self.s3 = self._stage(B*2,  B*4,  self.NUM_BLOCKS[2], stride=2)
        self.head = nn.Sequential(
            nn.AdaptiveAvgPool2d(1),
            nn.Flatten(),
            nn.Dropout(DROPOUT),
            nn.Linear(B * 4, 10),
        )

    def _stage(self, in_ch, out_ch, n, stride):
        layers = [ResBlock(in_ch, out_ch, stride)]
        for _ in range(n - 1):
            layers.append(ResBlock(out_ch, out_ch))
        return nn.Sequential(*layers)

    def forward(self, x):
        return self.head(self.s3(self.s2(self.s1(self.stem(x)))))


# ════════════════════════════════════════════════════════════════════════════
# DO NOT EDIT BELOW THIS LINE
# Training loop, data pipeline, and result output are fixed infrastructure.
# ════════════════════════════════════════════════════════════════════════════

NUM_EPOCHS = 20   # fixed training budget — do not change

import argparse
import json
import os
import sys
import time
from datetime import datetime
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("--debug",   action="store_true")
parser.add_argument("--dry-run", action="store_true")
_args = parser.parse_args()

def log(msg, level="INFO"):
    print(f"[{datetime.now().strftime('%H:%M:%S')}] [{level}] {msg}", flush=True)

def debug(msg):
    if _args.debug:
        log(msg, "DEBUG")

# ── Validate editable section ─────────────────────────────────────────────────
_errors = []
_VALID_OPT   = {"sgd", "adam", "adamw"}
_VALID_SCHED = {"none", "cosine", "step", "onecycle"}
_VALID_AUG   = {"none", "standard", "strong"}

if not (1e-5 <= LEARNING_RATE <= 2.0):   _errors.append(f"LEARNING_RATE {LEARNING_RATE} out of [1e-5, 2.0]")
if not (16 <= BATCH_SIZE <= 512):         _errors.append(f"BATCH_SIZE {BATCH_SIZE} out of [16, 512]")
if OPTIMIZER not in _VALID_OPT:           _errors.append(f"OPTIMIZER '{OPTIMIZER}' not in {_VALID_OPT}")
if SCHEDULER not in _VALID_SCHED:         _errors.append(f"SCHEDULER '{SCHEDULER}' not in {_VALID_SCHED}")
if AUGMENTATION not in _VALID_AUG:        _errors.append(f"AUGMENTATION '{AUGMENTATION}' not in {_VALID_AUG}")
if not (0.0 <= LABEL_SMOOTHING <= 0.5):  _errors.append(f"LABEL_SMOOTHING {LABEL_SMOOTHING} out of [0, 0.5]")
if not (0.0 <= DROPOUT <= 0.9):           _errors.append(f"DROPOUT {DROPOUT} out of [0, 0.9]")

if _errors:
    for e in _errors: log(e, "ERROR")
    sys.exit(1)

log("Hyperparameters:")
log(f"  lr={LEARNING_RATE} opt={OPTIMIZER} sched={SCHEDULER} aug={AUGMENTATION}")
log(f"  bs={BATCH_SIZE} epochs={NUM_EPOCHS} wd={WEIGHT_DECAY} ls={LABEL_SMOOTHING} dropout={DROPOUT}")
if NOTES: log(f"  notes: {NOTES}")

if _args.dry_run:
    log("Dry run — imports and config OK.")
    sys.exit(0)

# ── Imports ───────────────────────────────────────────────────────────────────
try:
    import torch
    import torch.optim as optim
    from torch.optim.lr_scheduler import CosineAnnealingLR, StepLR, OneCycleLR
    import torchvision
    import torchvision.transforms as transforms
except ImportError:
    log("PyTorch not installed: pip install torch torchvision", "ERROR"); sys.exit(1)

# ── Device ────────────────────────────────────────────────────────────────────
if torch.backends.mps.is_available():
    device = torch.device("mps")
    log("Device: Apple MPS")
elif torch.cuda.is_available():
    device = torch.device("cuda")
    log(f"Device: CUDA — {torch.cuda.get_device_name(0)}")
else:
    device = torch.device("cpu")
    log("Device: CPU")

# ── Model ─────────────────────────────────────────────────────────────────────
model = Model().to(device)
n_params = sum(p.numel() for p in model.parameters() if p.requires_grad)
log(f"Model: {n_params:,} trainable params")

# ── Data ──────────────────────────────────────────────────────────────────────
_data_dir = Path(__file__).parent / ".cifar10_data"
_data_dir.mkdir(exist_ok=True)

_MEAN = (0.4914, 0.4822, 0.4465)
_STD  = (0.2470, 0.2435, 0.2616)

if AUGMENTATION == "none":
    _train_tf = transforms.Compose([
        transforms.ToTensor(),
        transforms.Normalize(_MEAN, _STD),
    ])
elif AUGMENTATION == "standard":
    _train_tf = transforms.Compose([
        transforms.RandomCrop(32, padding=4),
        transforms.RandomHorizontalFlip(),
        transforms.ToTensor(),
        transforms.Normalize(_MEAN, _STD),
    ])
else:  # strong
    _train_tf = transforms.Compose([
        transforms.RandomCrop(32, padding=4),
        transforms.RandomHorizontalFlip(),
        transforms.ColorJitter(brightness=0.3, contrast=0.3, saturation=0.3, hue=0.1),
        transforms.RandomGrayscale(p=0.1),
        transforms.ToTensor(),
        transforms.Normalize(_MEAN, _STD),
        transforms.RandomErasing(p=0.5, scale=(0.02, 0.2)),
    ])

_val_tf = transforms.Compose([
    transforms.ToTensor(),
    transforms.Normalize(_MEAN, _STD),
])

log("Loading dataset (downloads on first run) …")
_train_set = torchvision.datasets.CIFAR10(str(_data_dir), train=True,  download=True, transform=_train_tf)
_val_set   = torchvision.datasets.CIFAR10(str(_data_dir), train=False, download=True, transform=_val_tf)
_train_loader = torch.utils.data.DataLoader(_train_set, batch_size=BATCH_SIZE, shuffle=True,  num_workers=0)
_val_loader   = torch.utils.data.DataLoader(_val_set,   batch_size=256,         shuffle=False, num_workers=0)
log(f"Train: {len(_train_set):,} | Val: {len(_val_set):,} | Batch: {BATCH_SIZE} | Aug: {AUGMENTATION}")

# ── Optimiser & scheduler ─────────────────────────────────────────────────────
_criterion = torch.nn.CrossEntropyLoss(label_smoothing=LABEL_SMOOTHING)

if OPTIMIZER == "sgd":
    _optimizer = optim.SGD(model.parameters(), lr=LEARNING_RATE, momentum=MOMENTUM,
                           weight_decay=WEIGHT_DECAY, nesterov=True)
elif OPTIMIZER == "adam":
    _optimizer = optim.Adam(model.parameters(), lr=LEARNING_RATE, weight_decay=WEIGHT_DECAY)
else:
    _optimizer = optim.AdamW(model.parameters(), lr=LEARNING_RATE, weight_decay=WEIGHT_DECAY)

if SCHEDULER == "cosine":
    _scheduler = CosineAnnealingLR(_optimizer, T_max=NUM_EPOCHS, eta_min=1e-6)
elif SCHEDULER == "step":
    _scheduler = StepLR(_optimizer, step_size=max(1, NUM_EPOCHS // 3), gamma=0.1)
elif SCHEDULER == "onecycle":
    _scheduler = OneCycleLR(_optimizer, max_lr=LEARNING_RATE, epochs=NUM_EPOCHS,
                             steps_per_epoch=len(_train_loader))
else:
    _scheduler = None

log(f"Optimizer: {OPTIMIZER} | Scheduler: {SCHEDULER}")

# ── Training loop ─────────────────────────────────────────────────────────────
_epoch_history = []
_best_val_acc  = 0.0
_best_epoch    = 0
_t_start       = time.time()

for _epoch in range(1, NUM_EPOCHS + 1):
    model.train()
    _ep_start = time.time()

    for _bi, (_imgs, _lbls) in enumerate(_train_loader):
        _imgs, _lbls = _imgs.to(device), _lbls.to(device)
        _optimizer.zero_grad()
        _loss = _criterion(model(_imgs), _lbls)
        _loss.backward()
        _optimizer.step()
        if SCHEDULER == "onecycle":
            _scheduler.step()
        if _args.debug and _bi % 50 == 0:
            debug(f"  ep{_epoch} b{_bi}/{len(_train_loader)} loss={_loss.item():.4f}")

    model.eval()
    _tr_loss = _tr_correct = _tr_total = 0
    with torch.no_grad():
        for _imgs, _lbls in _train_loader:
            _imgs, _lbls = _imgs.to(device), _lbls.to(device)
            _out = model(_imgs)
            _tr_loss    += _criterion(_out, _lbls).item() * _imgs.size(0)
            _tr_correct += (_out.argmax(1) == _lbls).sum().item()
            _tr_total   += _imgs.size(0)
    _train_acc  = _tr_correct / _tr_total
    _train_loss = _tr_loss / _tr_total

    _vl_loss = _vl_correct = _vl_total = 0
    with torch.no_grad():
        for _imgs, _lbls in _val_loader:
            _imgs, _lbls = _imgs.to(device), _lbls.to(device)
            _out = model(_imgs)
            _vl_loss    += _criterion(_out, _lbls).item() * _imgs.size(0)
            _vl_correct += (_out.argmax(1) == _lbls).sum().item()
            _vl_total   += _imgs.size(0)
    _val_acc  = _vl_correct / _vl_total
    _val_loss = _vl_loss / _vl_total
    _elapsed  = time.time() - _ep_start

    if _scheduler and SCHEDULER != "onecycle":
        _scheduler.step()

    _cur_lr = _optimizer.param_groups[0]["lr"]
    _marker = ""
    if _val_acc > _best_val_acc:
        _best_val_acc = _val_acc
        _best_epoch   = _epoch
        _marker = " ◀ best"

    log(f"Epoch {_epoch:3d}/{NUM_EPOCHS} | "
        f"train {_train_loss:.4f}/{_train_acc:.4f} | "
        f"val {_val_loss:.4f}/{_val_acc:.4f} | "
        f"lr={_cur_lr:.6f} | {_elapsed:.1f}s{_marker}")

    _epoch_history.append({
        "epoch": _epoch,
        "train_loss": round(_train_loss, 6), "train_acc": round(_train_acc, 6),
        "val_loss":   round(_val_loss, 6),   "val_acc":   round(_val_acc, 6),
        "lr": round(_cur_lr, 8), "elapsed_s": round(_elapsed, 2),
    })

_total_time = time.time() - _t_start
log(f"Done in {_total_time:.1f}s | best val_acc={_best_val_acc:.4f} @ epoch {_best_epoch}")

# ── Save results ──────────────────────────────────────────────────────────────
_run_id = f"run_{datetime.now().strftime('%Y%m%d_%H%M%S')}"
_results = {
    "run_id": _run_id,
    "timestamp": datetime.now().isoformat(),
    "hyperparameters": {
        "learning_rate": LEARNING_RATE, "batch_size": BATCH_SIZE,
        "optimizer": OPTIMIZER, "weight_decay": WEIGHT_DECAY,
        "momentum": MOMENTUM, "scheduler": SCHEDULER,
        "augmentation": AUGMENTATION, "label_smoothing": LABEL_SMOOTHING,
        "dropout": DROPOUT, "num_epochs": NUM_EPOCHS, "notes": NOTES,
    },
    "model": {"total_params": n_params},
    "device": str(device),
    "metrics": {
        "final_val_accuracy": round(_epoch_history[-1]["val_acc"], 6),
        "final_val_loss":     round(_epoch_history[-1]["val_loss"], 6),
        "best_val_accuracy":  round(_best_val_acc, 6),
        "best_epoch":         _best_epoch,
        "train_time_seconds": round(_total_time, 2),
        "total_params":       n_params,
    },
    "epoch_history": _epoch_history,
}

_results_dir = Path(__file__).parent / "results"
_results_dir.mkdir(exist_ok=True)
_out_file = _results_dir / f"{_run_id}.json"
_out_file.write_text(json.dumps(_results, indent=2))
log(f"Saved → {_out_file}")

_latest = _results_dir / "latest.json"
if _latest.is_symlink() or _latest.exists(): _latest.unlink()
_latest.symlink_to(_out_file.name)

# ── Machine-readable summary ──────────────────────────────────────────────────
_summary = {
    "val_accuracy":      round(_epoch_history[-1]["val_acc"], 6),
    "val_loss":          round(_epoch_history[-1]["val_loss"], 6),
    "best_val_accuracy": round(_best_val_acc, 6),
    "best_epoch":        _best_epoch,
    "train_time_s":      round(_total_time, 2),
    "total_params":      n_params,
    "results_file":      str(_out_file),
}
print(f"\nRESULT_JSON: {json.dumps(_summary)}", flush=True)
