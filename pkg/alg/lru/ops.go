package lru

// Get retrieves a value from the cache.
// If a Bloom filter is configured, definite misses are short-circuited
// without lock acquisition.
func (c *Cache[K, V]) Get(key K) (V, bool) {
	if c.filter != nil && !c.filter.Test(c.keyToBytes(key)) {
		c.bloomFiltered.Add(1)
		c.misses.Add(1)

		var zero V

		return zero, false
	}

	c.mu.Lock()
	defer c.mu.Unlock()

	ent, ok := c.entries[key]
	if !ok {
		c.misses.Add(1)

		var zero V

		return zero, false
	}

	c.hits.Add(1)

	ent.accessCount++
	c.moveToFront(ent)

	return ent.value, true
}

// Put adds or updates a key-value pair in the cache.
// If the value exceeds the maximum cache size, it is silently skipped.
func (c *Cache[K, V]) Put(key K, value V) {
	valSize := c.valueSize(value)

	// Reject values larger than the entire cache.
	if c.maxSize > 0 && valSize > c.maxSize {
		return
	}

	if c.cloneFunc != nil {
		value = c.cloneFunc(value)
	}

	c.mu.Lock()
	c.putLocked(key, value, valSize)
	c.mu.Unlock()
}

// putLocked inserts or updates an entry under the write lock.
func (c *Cache[K, V]) putLocked(key K, value V, valSize int64) {
	// Update existing entry.
	if ent, ok := c.entries[key]; ok {
		c.curSize -= ent.size
		ent.value = value
		ent.size = valSize
		ent.accessCount++
		c.curSize += valSize
		c.moveToFront(ent)

		return
	}

	c.evictUntilFits(valSize)

	// If still can't fit after full eviction, skip.
	if c.maxSize > 0 && c.curSize+valSize > c.maxSize {
		return
	}

	ent := &entry[K, V]{
		key:         key,
		value:       value,
		size:        valSize,
		accessCount: 1,
	}

	c.entries[key] = ent
	c.curSize += valSize
	c.addToFront(ent)

	if c.filter != nil {
		c.filter.Add(c.keyToBytes(key))
	}
}

// GetMulti retrieves multiple values from the cache.
// Returns a map of found key-value pairs and a slice of missing keys.
func (c *Cache[K, V]) GetMulti(keys []K) (found map[K]V, missing []K) {
	found = make(map[K]V)
	missing = make([]K, 0)

	// Partition keys using Bloom filter if available.
	candidates := c.bloomPartition(keys, &missing)

	if len(candidates) == 0 {
		return found, missing
	}

	c.mu.Lock()
	defer c.mu.Unlock()

	for _, key := range candidates {
		if ent, ok := c.entries[key]; ok {
			c.hits.Add(1)

			ent.accessCount++
			c.moveToFront(ent)
			found[key] = ent.value
		} else {
			c.misses.Add(1)

			missing = append(missing, key)
		}
	}

	return found, missing
}

// PutMulti adds multiple key-value pairs to the cache.
// Uses a single lock acquisition for the entire batch.
func (c *Cache[K, V]) PutMulti(items map[K]V) {
	c.mu.Lock()
	defer c.mu.Unlock()

	for key, value := range items {
		valSize := c.valueSize(value)

		if c.maxSize > 0 && valSize > c.maxSize {
			continue
		}

		if c.cloneFunc != nil {
			value = c.cloneFunc(value)
		}

		c.putLocked(key, value, valSize)
	}
}

// Clear removes all entries and resets the Bloom filter.
func (c *Cache[K, V]) Clear() {
	c.mu.Lock()
	defer c.mu.Unlock()

	c.entries = make(map[K]*entry[K, V])
	c.head = nil
	c.tail = nil
	c.curSize = 0

	if c.filter != nil {
		c.filter.Reset()
	}
}

// valueSize returns the size of a value using the configured size function,
// or 1 if no size function is configured.
func (c *Cache[K, V]) valueSize(value V) int64 {
	if c.sizeFunc != nil {
		return c.sizeFunc(value)
	}

	return 1
}

// bloomPartition separates keys into Bloom-positive candidates and
// definite misses. Without a Bloom filter, all keys are candidates.
func (c *Cache[K, V]) bloomPartition(keys []K, missing *[]K) []K {
	if c.filter == nil {
		return keys
	}

	candidates := make([]K, 0, len(keys))

	for _, key := range keys {
		if c.filter.Test(c.keyToBytes(key)) {
			candidates = append(candidates, key)
		} else {
			c.bloomFiltered.Add(1)
			c.misses.Add(1)

			*missing = append(*missing, key)
		}
	}

	return candidates
}

// evictUntilFits removes entries until the new value fits.
func (c *Cache[K, V]) evictUntilFits(valSize int64) {
	// Count-based eviction.
	for c.maxEntries > 0 && len(c.entries) >= c.maxEntries && c.tail != nil {
		c.evictOne()
	}

	// Size-based eviction.
	for c.maxSize > 0 && c.curSize+valSize > c.maxSize && c.tail != nil {
		c.evictOne()
	}
}

// evictOne removes one entry using cost-based sampling or simple LRU.
func (c *Cache[K, V]) evictOne() {
	if c.costFunc != nil && c.sampleSize > 0 {
		c.evictLowestCost()

		return
	}

	c.evictTail()
}

// evictTail removes the least recently used entry.
func (c *Cache[K, V]) evictTail() {
	if c.tail == nil {
		return
	}

	victim := c.tail
	c.removeFromList(victim)
	delete(c.entries, victim.key)
	c.curSize -= victim.size
}

// evictLowestCost samples entries from the tail and evicts the lowest-cost one.
func (c *Cache[K, V]) evictLowestCost() {
	if c.tail == nil {
		return
	}

	victim := c.tail
	lowestCost := c.costFunc(victim.accessCount, victim.size)

	count := 1
	ent := victim.prev

	for ent != nil && count < c.sampleSize {
		cost := c.costFunc(ent.accessCount, ent.size)
		if cost < lowestCost {
			lowestCost = cost
			victim = ent
		}

		count++
		ent = ent.prev
	}

	c.removeFromList(victim)
	delete(c.entries, victim.key)
	c.curSize -= victim.size
}

// moveToFront moves an entry to the head of the LRU list.
func (c *Cache[K, V]) moveToFront(ent *entry[K, V]) {
	if ent == c.head {
		return
	}

	c.removeFromList(ent)
	c.addToFront(ent)
}

// addToFront adds an entry at the head of the LRU list.
func (c *Cache[K, V]) addToFront(ent *entry[K, V]) {
	ent.prev = nil
	ent.next = c.head

	if c.head != nil {
		c.head.prev = ent
	}

	c.head = ent

	if c.tail == nil {
		c.tail = ent
	}
}

// removeFromList removes an entry from the LRU list.
func (c *Cache[K, V]) removeFromList(ent *entry[K, V]) {
	if ent.prev != nil {
		ent.prev.next = ent.next
	} else {
		c.head = ent.next
	}

	if ent.next != nil {
		ent.next.prev = ent.prev
	} else {
		c.tail = ent.prev
	}
}
