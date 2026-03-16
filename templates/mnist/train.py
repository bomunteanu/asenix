"""MNIST digit classification — Asenix agent training script.

Edit everything in the AGENT-EDITABLE SECTION.
Do not touch anything below DO NOT EDIT.
"""

# ── AGENT-EDITABLE SECTION ────────────────────────────────────────────────────

LEARNING_RATE = 0.001
BATCH_SIZE    = 128
OPTIMIZER     = "adam"      # "adam" | "sgd" | "adamw"
WEIGHT_DECAY  = 1e-4
MOMENTUM      = 0.9         # SGD only
CONV_FILTERS  = [32, 64]    # filters per conv layer (exactly 2 values)
HIDDEN_SIZE   = 128         # FC hidden layer width
DROPOUT       = 0.25
ACTIVATION    = "relu"      # "relu" | "gelu" | "silu"
SCHEDULER     = "none"      # "none" | "cosine" | "onecycle"
NOTES         = "baseline: Adam + 2-layer CNN"


import torch
import torch.nn as nn


class Model(nn.Module):
    """Two conv layers → global max pool → FC → 10 classes.
    You may freely restructure this — add layers, skip connections, etc.
    The only contract: input shape (B, 1, 28, 28), output shape (B, 10).
    """
    def __init__(self):
        super().__init__()
        act = {"relu": nn.ReLU, "gelu": nn.GELU, "silu": nn.SiLU}[ACTIVATION]
        self.features = nn.Sequential(
            nn.Conv2d(1, CONV_FILTERS[0], 3, padding=1), act(),
            nn.Conv2d(CONV_FILTERS[0], CONV_FILTERS[1], 3, padding=1), act(),
            nn.MaxPool2d(2),
            nn.Dropout2d(DROPOUT),
        )
        self.head = nn.Sequential(
            nn.Flatten(),
            nn.Linear(CONV_FILTERS[1] * 14 * 14, HIDDEN_SIZE), act(),
            nn.Dropout(DROPOUT),
            nn.Linear(HIDDEN_SIZE, 10),
        )

    def forward(self, x):
        return self.head(self.features(x))


# ── DO NOT EDIT BELOW THIS LINE ───────────────────────────────────────────────

import os, json, time, argparse
import torch.optim as optim
from torch.utils.data import DataLoader
from torchvision import datasets, transforms

NUM_EPOCHS = 5   # fixed training budget


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    # Validate editable constants
    assert len(CONV_FILTERS) == 2, "CONV_FILTERS must have exactly 2 values"
    assert OPTIMIZER in ("adam", "sgd", "adamw"), f"Unknown optimizer: {OPTIMIZER}"
    assert ACTIVATION in ("relu", "gelu", "silu"), f"Unknown activation: {ACTIVATION}"
    assert SCHEDULER in ("none", "cosine", "onecycle"), f"Unknown scheduler: {SCHEDULER}"

    device = (
        torch.device("mps")  if torch.backends.mps.is_available() else
        torch.device("cuda") if torch.cuda.is_available() else
        torch.device("cpu")
    )
    print(f"device: {device}  epochs: {NUM_EPOCHS}  notes: {NOTES}")

    transform = transforms.Compose([
        transforms.ToTensor(),
        transforms.Normalize((0.1307,), (0.3081,)),
    ])
    train_ds = datasets.MNIST("data", train=True,  download=True, transform=transform)
    val_ds   = datasets.MNIST("data", train=False, download=True, transform=transform)
    train_dl = DataLoader(train_ds, batch_size=BATCH_SIZE, shuffle=True,  num_workers=0)
    val_dl   = DataLoader(val_ds,   batch_size=512,        shuffle=False, num_workers=0)

    model = Model().to(device)
    total_params = sum(p.numel() for p in model.parameters() if p.requires_grad)
    print(f"params: {total_params:,}")

    if args.dry_run:
        out = model(torch.zeros(2, 1, 28, 28, device=device))
        assert out.shape == (2, 10), f"unexpected output shape: {out.shape}"
        print("dry-run OK")
        return

    opt_cls = {"adam": optim.Adam, "adamw": optim.AdamW, "sgd": optim.SGD}[OPTIMIZER]
    opt_kw  = {"lr": LEARNING_RATE, "weight_decay": WEIGHT_DECAY}
    if OPTIMIZER == "sgd":
        opt_kw["momentum"] = MOMENTUM
    optimizer = opt_cls(model.parameters(), **opt_kw)

    steps = len(train_dl)
    if SCHEDULER == "cosine":
        sched = optim.lr_scheduler.CosineAnnealingLR(optimizer, T_max=NUM_EPOCHS)
    elif SCHEDULER == "onecycle":
        sched = optim.lr_scheduler.OneCycleLR(
            optimizer, max_lr=LEARNING_RATE * 10, steps_per_epoch=steps, epochs=NUM_EPOCHS
        )
    else:
        sched = None

    criterion = nn.CrossEntropyLoss()
    t0 = time.time()

    for epoch in range(1, NUM_EPOCHS + 1):
        model.train()
        tr_loss = tr_correct = tr_total = 0
        for x, y in train_dl:
            x, y = x.to(device), y.to(device)
            optimizer.zero_grad()
            out = model(x)
            loss = criterion(out, y)
            loss.backward()
            optimizer.step()
            if SCHEDULER == "onecycle" and sched:
                sched.step()
            tr_loss    += loss.item() * len(x)
            tr_correct += out.argmax(1).eq(y).sum().item()
            tr_total   += len(x)
        if SCHEDULER == "cosine" and sched:
            sched.step()

        model.eval()
        vl = vc = vt = 0
        with torch.no_grad():
            for x, y in val_dl:
                x, y = x.to(device), y.to(device)
                out = model(x)
                vl += criterion(out, y).item() * len(x)
                vc += out.argmax(1).eq(y).sum().item()
                vt += len(x)
        val_acc  = vc / vt
        val_loss = vl / vt
        print(f"epoch {epoch}/{NUM_EPOCHS}  train_acc={tr_correct/tr_total:.4f}  "
              f"val_acc={val_acc:.4f}  val_loss={val_loss:.4f}")

    elapsed = time.time() - t0
    os.makedirs("results", exist_ok=True)
    result = {
        "val_accuracy": round(val_acc, 6),
        "val_loss":     round(val_loss, 6),
        "train_time_s": round(elapsed, 1),
        "total_params": total_params,
        "notes":        NOTES,
    }
    with open("results/latest.json", "w") as f:
        json.dump(result, f, indent=2)
    print(f"RESULT_JSON: {json.dumps(result)}")


if __name__ == "__main__":
    main()
