// Package main — hashring.go
//
// HashRing implements consistent hashing for the Dynamo simulation.
//
// How it works:
//  1. Each physical node is mapped to node.VnodeCount positions ("virtual nodes")
//     on a uint32 hash ring.  VnodeCount is set proportionally to the node's
//     hardware capacity by BuildTopology, so more powerful nodes occupy a
//     larger slice of the key-space.
//  2. A key is hashed to a point on the ring, and we walk clockwise to
//     find R distinct *physical* nodes. These become the key's replicas.
//  3. The hash function uses SHA-256, truncated to 4 bytes → uint32.

package main

import (
	"crypto/sha256"
	"encoding/binary"
	"fmt"
	"sort"
)

// ------------------- types -------------------

// vnode is a single point on the hash ring.
// It stores the hash position and the physical node it maps to.
type vnode struct {
	hash   uint32
	nodeID int
}

// HashRing holds the sorted ring of virtual nodes and a lookup
// table from node-ID → *Node for quick access.
type HashRing struct {
	ring    []vnode       // sorted by hash
	nodeMap map[int]*Node // nodeID → *Node
}

// ------------------- constructor -------------------

// NewHashRing builds a consistent-hash ring from the given nodes.
//
// The number of virtual positions each node occupies is taken directly from
// node.VnodeCount, which was set by BuildTopology proportionally to that
// node's hardware capacity.  This means high-capacity nodes naturally own
// more of the ring (and therefore more keys) than low-capacity ones.
func NewHashRing(nodes []*Node) *HashRing {
	hr := &HashRing{
		nodeMap: make(map[int]*Node, len(nodes)),
	}

	for _, n := range nodes {
		hr.nodeMap[n.ID] = n
		// Create node.VnodeCount virtual entries for this physical node.
		for v := 0; v < n.VnodeCount; v++ {
			label := fmt.Sprintf("node-%d-vn-%d", n.ID, v)
			hr.ring = append(hr.ring, vnode{
				hash:   hashKey(label),
				nodeID: n.ID,
			})
		}
	}

	// Sort the ring by hash so binary search works.
	sort.Slice(hr.ring, func(i, j int) bool {
		return hr.ring[i].hash < hr.ring[j].hash
	})

	return hr
}

// ------------------- core API -------------------

// GetReplicaNodes returns up to r distinct physical nodes responsible
// for the given key. It finds the first position on the ring whose hash
// is ≥ the key's hash, then walks clockwise, skipping duplicate physical
// nodes, until r unique nodes are collected (or the ring is exhausted).
func (hr *HashRing) GetReplicaNodes(key string, r int) []*Node {
	if len(hr.ring) == 0 {
		return nil
	}

	h := hashKey(key)

	// Binary-search for the first vnode with hash ≥ h.
	start := sort.Search(len(hr.ring), func(i int) bool {
		return hr.ring[i].hash >= h
	})

	seen := make(map[int]bool, r)
	var replicas []*Node

	// Walk the ring (wrapping around) until we have r distinct nodes.
	for i := 0; i < len(hr.ring) && len(replicas) < r; i++ {
		idx := (start + i) % len(hr.ring)
		nid := hr.ring[idx].nodeID

		if seen[nid] {
			continue
		}
		seen[nid] = true
		replicas = append(replicas, hr.nodeMap[nid])
	}

	return replicas
}

// ------------------- helpers -------------------

// hashKey hashes an arbitrary string to a uint32 using SHA-256.
// We take the first 4 bytes of the digest for a uniform 32-bit value.
func hashKey(key string) uint32 {
	sum := sha256.Sum256([]byte(key))
	return binary.BigEndian.Uint32(sum[:4])
}
