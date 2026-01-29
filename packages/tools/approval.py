"""
Approval utilities.
"""

from __future__ import annotations

import os


def is_approved(token: str | None) -> bool:
    """
    Stage 4B minimal approval mechanism.
    Real approval workflow comes later (UI prompt -> audit event -> approval store).
    """
    expected = os.environ.get("PIEBOT_APPROVAL_TOKEN", "").strip()
    if not expected:
        return False
    if not token:
        return False
    return token.strip() == expected