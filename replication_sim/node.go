// Package main — node.go
//
// Node represents a single server in the Dynamo-style cluster.
// Each node has a unique ID, an alive/dead status flag, and a local
// key-value store (presence map). The store records which keys have
// been replicated to this node.
//
// Additional fields (Capacity, VnodeCount, FailProb) support the
// topology-aware simulation:
//
//   - Capacity   – hardware capacity units drawn from a Discrete Uniform
//                  distribution over [MinCapacity, MaxCapacity].  Higher
//                  capacity nodes handle more concurrent requests.
//
//   - VnodeCount – number of virtual positions this node occupies on the
//                  consistent-hash ring.  Computed as:
//                      round(BaseVnodes × capacity / meanCapacity)
//                  so more powerful nodes own a proportionally larger
//                  slice of the key-space.
//
//   - FailProb   – individual node failure probability drawn from a
//                  Uniform distribution centred at BaseFailProb ± FailSpread
//                  at build time by BuildTopology.

package main

import "fmt"

// Node models a single storage node in the cluster.
type Node struct {
	ID         int             // unique, hashable identifier
	Alive      bool            // true = reachable, false = failed
	Store      map[string]bool // local key presence map

	// ── topology & ring fields ──────────────────────────────────────────────
	Capacity   int     // relative hardware capacity units
	VnodeCount int     // number of virtual nodes on the hash ring
	FailProb   float64 // individual failure probability (set by BuildTopology)
}

// NewNode creates a node with the given id, capacity, vnode count, and
// individual failure probability. The node starts alive with an empty store.
func NewNode(id, capacity, vnodeCount int, failProb float64) *Node {
	return &Node{
		ID:         id,
		Alive:      true,
		Store:      make(map[string]bool),
		Capacity:   capacity,
		VnodeCount: vnodeCount,
		FailProb:   failProb,
	}
}

// Put stores a key on this node.
func (n *Node) Put(key string) {
	n.Store[key] = true
}

// Get checks whether a key exists on this node.
// Returns false if the key is absent.
func (n *Node) Get(key string) bool {
	return n.Store[key]
}

// Kill marks the node as failed / unreachable.
func (n *Node) Kill() {
	n.Alive = false
}

// Revive marks the node as alive again.
func (n *Node) Revive() {
	n.Alive = true
}

// String returns a human-readable representation for debugging.
func (n *Node) String() string {
	status := "alive"
	if !n.Alive {
		status = "dead"
	}
	return fmt.Sprintf("Node{id=%d, cap=%d, vnodes=%d, failP=%.3f, status=%s, keys=%d}",
		n.ID, n.Capacity, n.VnodeCount, n.FailProb, status, len(n.Store))
}
