// Package main — node.go
//
// Node represents a single server in the Dynamo-style cluster.
// Each node has a unique ID, an alive/dead status flag, and a local
// key-value store (presence map). The store records which keys have
// been replicated to this node.

package main

import "fmt"

// Node models a single storage node in the cluster.
type Node struct {
	ID    int             // unique, hashable identifier
	Alive bool            // true = reachable, false = failed
	Store map[string]bool // local key presence map
}

// NewNode creates a node with the given id, marks it alive,
// and initialises an empty store.
func NewNode(id int) *Node {
	return &Node{
		ID:    id,
		Alive: true,
		Store: make(map[string]bool),
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
	return fmt.Sprintf("Node{id=%d, status=%s, keys=%d}", n.ID, status, len(n.Store))
}
