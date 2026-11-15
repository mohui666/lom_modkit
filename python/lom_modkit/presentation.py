"""Optional helpers that return the same Show/Hide/Parallel node objects."""
from copy import deepcopy

from . import Hide, Node, Parallel, Show, _identifier


class NodeIds:
    """Generate deterministic chapter-local IDs, skipping explicitly reserved IDs."""
    def __init__(self, existing=()):
        self._used = set()
        self._next = {}
        for value in existing:
            node_id = value if isinstance(value, str) else (
                value.id if isinstance(value, Node) else value["id"])
            self.reserve(node_id)

    def reserve(self, node_id):
        _identifier(node_id, "NodeIds ID")
        self._used.add(node_id)
        return node_id

    def __call__(self, prefix="node"):
        _identifier(prefix, "NodeIds prefix")
        number = self._next.get(prefix, 1)
        while f"{prefix}{number}" in self._used:
            number += 1
        self._next[prefix] = number + 1
        return self.reserve(f"{prefix}{number}")


class Staging:
    """Author-declared staging; never infers portrait cropping or live game state."""
    def __init__(self, ids=None):
        if ids is not None and not isinstance(ids, NodeIds):
            raise TypeError("Staging.ids must be a NodeIds instance")
        self.ids = ids if ids is not None else NodeIds()
        self._placements = {}

    def _node(self, constructor, node_id, fields):
        node = constructor(id=node_id, **fields)
        if node_id is None:
            return node.replace(id=self.ids(node.kind))
        self.ids.reserve(node_id)
        return node

    def remember(self, show):
        """Remember an explicit Show as a restore point, without adding a node."""
        node = show if isinstance(show, Node) else Node.from_dict(show)
        if node.kind != "show":
            raise ValueError("Staging.remember needs a Show node")
        fields = deepcopy(node.fields)
        for key in ("goto", "_editor", "wait_after"):
            fields.pop(key, None)
        self._placements[fields["character"]] = fields

    def enter(self, character, position, *, id=None, **fields):
        """Show and remember its declared position/portrait/facing/scale."""
        fields = {"wait_for_completion": True, **fields,
                  "character": character, "position": position}
        node = self._node(Show, id, fields)
        self.remember(node)
        return node

    def exit(self, character, *, id=None, **fields):
        return self._node(Hide, id, {"wait_for_completion": True, **fields,
                                     "character": character})

    def restore(self, character, *, id=None, **fields):
        """Return a Show using the last enter/remember declaration plus overrides."""
        if character not in self._placements:
            raise ValueError(f"No declared placement for {character!r}; call enter or remember first")
        return self._node(Show, id, {**deepcopy(self._placements[character]), **fields})

    def together(self, *actions, id=None):
        """Join 2–32 existing node actions; reject flow/goto rather than discarding it."""
        if not 2 <= len(actions) <= 32:
            raise ValueError("Staging.together needs 2..32 actions")
        values = [(action if isinstance(action, Node) else Node.from_dict(action)).action()
                  for action in actions]
        return self._node(Parallel, id, {"actions": values})


__all__ = ["NodeIds", "Staging"]
