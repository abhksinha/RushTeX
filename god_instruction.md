Based on those ten principles, I would turn them into these **technology-independent engineering rules**:

1. **Borrow or reference existing data whenever possible.** Avoid copying data merely to pass it between functions, modules, or layers.

2. **Transfer ownership instead of duplicating data.** When one component no longer needs an object or buffer, hand it to the next component rather than cloning it.

3. **Process data in place when safe and appropriate.** Transform existing storage instead of allocating an input buffer, intermediate buffer, and output buffer.

4. **Allocate once and reuse aggressively on hot paths.** Preallocate buffers, pools, queues, scratch space, and other frequently used storage when workloads permit.

5. **Use compact native/binary representations on performance-critical paths.** Avoid unnecessary serialization, parsing, object conversion, and verbose intermediate formats between internal layers.

6. **Design memory layout around the access pattern.** Use SoA when repeatedly scanning selected fields, AoS when consuming complete records, and hybrid layouts when appropriate. Optimize for cache locality rather than ideology.

7. **Keep pipelines bounded.** Put explicit capacity limits on queues, buffers, caches, and pending work. Apply backpressure rather than allowing uncontrolled memory growth.

8. **Remove maintenance work from latency-critical paths.** Defer cleanup, compaction, logging, analytics, cache maintenance, and other nonessential work unless the immediate result requires them.

9. **Optimize hot code selectively.** Spend compiler optimization, SIMD, specialized algorithms, memory, parallelism, and engineering complexity where profiling demonstrates significant execution cost. Keep cold paths simple.

10. **Optimize startup and critical paths around necessity.** Before every operation ask: **“Must this happen now?”** If not, defer it, lazy-load it, precompute it, cache it, batch it, or eliminate it.

The overarching rule connecting all ten is:

> **Do less work, move less data, allocate less memory, touch less memory, and perform nonessential work later. Then measure to verify that the optimization actually helped.**

