# SPDX-License-Identifier: AGPL-3.0-only
"""State transitions for a two-slot firmware selector.

The boot glue has to durably commit the returned state before handing off to a
trial slot. A trusted loader, rather than a caller-supplied slot name, must
measure and identify the root image before the corresponding confirmation.
"""
from dataclasses import dataclass, replace

SLOTS = frozenset(("a", "b"))
TRIAL_ATTEMPTS = 2


@dataclass(frozen=True)
class BootState:
    confirmed: str
    # Only verified images are eligible. The update writer clears the inactive
    # slot before overwriting it, then registers it after complete readback.
    verified: frozenset[str]
    pending: str | None = None
    tries_left: int = 0
    last_trial: str | None = None

    def __post_init__(self):
        if not self.verified <= SLOTS or self.confirmed not in self.verified:
            raise ValueError("confirmed slot must be verified")
        if self.pending is None:
            if self.tries_left or self.last_trial is not None:
                raise ValueError("unexpected trial state")
        elif (self.pending == self.confirmed or self.pending not in self.verified
              or self.last_trial not in (None, self.pending)
              or not 0 <= self.tries_left <= TRIAL_ATTEMPTS
              or (self.last_trial is None) != (self.tries_left == TRIAL_ATTEMPTS)):
            raise ValueError("invalid trial state")

    def begin_write(self, slot):
        if slot not in SLOTS or slot == self.confirmed or self.pending is not None:
            raise ValueError("only the inactive slot can be written outside a trial")
        return replace(self, verified=self.verified - {slot})

    def verify_inactive(self, slot):
        if slot not in SLOTS or slot == self.confirmed or self.pending is not None:
            raise ValueError("cannot register this slot")
        return replace(self, verified=self.verified | {slot})

    def schedule(self, slot):
        """Schedule an installed image for trial, including manual rollback."""
        if slot == self.confirmed or slot not in self.verified or self.pending is not None:
            raise ValueError("requires a verified inactive slot and no existing trial")
        return replace(self, pending=slot, tries_left=TRIAL_ATTEMPTS)

    def next_boot(self):
        if self.pending is None:
            return self.confirmed, self
        if self.tries_left:
            return self.pending, replace(self, tries_left=self.tries_left - 1, last_trial=self.pending)
        return self.confirmed, replace(self, verified=self.verified - {self.pending}, pending=None, last_trial=None)

    def confirm(self, running_slot):
        if self.last_trial is None or running_slot != self.last_trial:
            raise ValueError("confirmation must match the booted trial slot")
        return replace(self, confirmed=running_slot, pending=None, tries_left=0, last_trial=None)
