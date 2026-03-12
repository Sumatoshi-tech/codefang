package pipeline

import "context"

// minBuffer is the minimum capacity for the internal jobs channel.
const minBuffer = 1

// RunPC is a producer-consumer micro-skeleton that owns the goroutine topology:
// channel creation, goroutine spawning, and orderly shutdown.
//
// Type parameters:
//   - In:  the input consumed by the producer
//   - Out: the output emitted by the consumer
//   - Job: the internal work item flowing from producer to consumer
//
// The Produce function reads from in and writes jobs to the jobs channel.
// The Consume function reads jobs and writes results to the out channel.
// Neither function should close its output channel; RunPC handles that.
type RunPC[In, Out, Job any] struct {
	// Buffer sets the capacity of the internal jobs channel.
	// Values below 1 are clamped to 1.
	Buffer int

	// Produce reads the input and sends work items on the jobs channel.
	Produce func(ctx context.Context, in In, jobs chan<- Job)

	// Consume reads work items from jobs and sends results on out.
	Consume func(ctx context.Context, jobs <-chan Job, out chan<- Out)
}

// Run starts the producer and consumer goroutines and returns the output channel.
// The jobs channel is closed after Produce returns. The output channel is closed
// after Consume returns.
func (r RunPC[In, Out, Job]) Run(ctx context.Context, in In) <-chan Out {
	buf := max(r.Buffer, minBuffer)

	jobs := make(chan Job, buf)
	out := make(chan Out)

	go func() {
		defer close(jobs)

		r.Produce(ctx, in, jobs)
	}()

	go func() {
		defer close(out)

		r.Consume(ctx, jobs, out)
	}()

	return out
}
