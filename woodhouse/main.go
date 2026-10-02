package main

// Test

import (
	"context"
	"encoding/json"
	"log"
	"os"
	"os/signal"
	"time"

	"github.com/nats-io/nats.go"
	"github.com/nats-io/nats.go/jetstream"
)

type BookmarkJob struct {
	ID  string `json:"id"`
	URL string `json:"url"`
}

type CompletedEvent struct {
	ID         string `json:"id"`
	StorageURL string `json:"storage_url"`
}

func main() {
	natsURL := os.Getenv("NATS_URL")
	log.Printf("[DEBUG] Attempting to connect to NATS at: %q", natsURL)

	nc, err := nats.Connect(natsURL)
	if err != nil {
		log.Fatalf("[FATAL] Failed to connect to NATS at %s: %v", natsURL, err)
	}
	defer nc.Close()

	// 1. Initialize the modern JetStream API
	js, err := jetstream.New(nc)
	if err != nil {
		log.Fatalf("[FATAL] Failed to initialize JetStream: %v", err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	// 2. Declare the Stream
	stream, err := js.CreateOrUpdateStream(ctx, jetstream.StreamConfig{
		Name:      "WOODHOUSE_EVENTS",
		Subjects:  []string{"woodhouse.>"},
		Storage:   jetstream.FileStorage,
		Retention: jetstream.LimitsPolicy,
		Discard:   jetstream.DiscardOld,
	})
	if err != nil {
		log.Fatalf("[FATAL] Failed to verify stream: %v", err)
	}
	log.Printf("[INFO] Stream %q successfully initialized.", stream.CachedInfo().Config.Name)

	// 3. Declare the Consumer (replaces QueueSubscribe)
	cons, err := stream.CreateOrUpdateConsumer(ctx, jetstream.ConsumerConfig{
		Durable:       "bookmark-workers",
		DeliverGroup:  "bookmark-workers", // This creates the queue group behavior
		FilterSubject: "woodhouse.bookmark.pending",
		MaxDeliver:    3,
		AckPolicy:     jetstream.AckExplicitPolicy,
	})
	if err != nil {
		log.Fatalf("[FATAL] Failed to create consumer: %v", err)
	}

	// 4. Start consuming (msg is now jetstream.Msg, not *nats.Msg)
	cc, err := cons.Consume(func(msg jetstream.Msg) {
		// Extract metadata to track delivery attempts
		meta, metaErr := msg.Metadata()
		isFinalAttempt := metaErr == nil && meta.NumDelivered >= 3

		var job BookmarkJob
		if err := json.Unmarshal(msg.Data(), &job); err != nil {
			log.Printf("Invalid payload, terminating message: %v", err)
			msg.Term()
			return
		}

		// Mocking your generation failure
		err = generateMarkdownMock()
		if err != nil {
			if isFinalAttempt {
				log.Printf("[DLQ] Max deliveries (3) reached for %s. Routing to DLQ.", job.URL)

				// 1. Publish to the DLQ subject
				_, pubErr := js.Publish(context.Background(), "woodhouse.bookmark.dlq", msg.Data())
				if pubErr != nil {
					log.Printf("CRITICAL: Failed to write to DLQ: %v", pubErr)
					msg.NakWithDelay(10 * time.Second) // Force retry if DLQ write fails
					return
				}

				// 2. Ack the original message so the consumer stops trying
				msg.Ack()
				return
			}

			log.Printf("Attempt %d failed for %s: %v", meta.NumDelivered, job.URL, err)
			msg.NakWithDelay(30 * time.Second)
			return
		}

		log.Printf("SUCCESS: Processed bookmark for %s", job.URL)
		// 1. Initialize the modern JetStream API
		js, err := jetstream.New(nc)
		if err != nil {
			log.Fatalf("[FATAL] Failed to initialize JetStream: %v", err)
		}

		// 2. Define the exact stream requirements
		cfg := jetstream.StreamConfig{
			Name:      "WOODHOUSE_EVENTS",
			Subjects:  []string{"woodhouse.>"},
			Storage:   jetstream.FileStorage,
			Retention: jetstream.LimitsPolicy,
			Discard:   jetstream.DiscardOld,
		}

		ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
		defer cancel()

		// 3. Declarative initialization (creates or patches the stream)
		stream, err := js.CreateOrUpdateStream(ctx, cfg)
		if err != nil {
			log.Fatalf("[FATAL] Failed to verify stream: %v", err)
		}

		log.Printf("[INFO] Stream %q successfully initialized.", stream.CachedInfo().Config.Name)

		publishCompletion(context.Background(), js, job.ID, "local://stdout-only")
		msg.Ack()
	})
	defer cc.Stop()

	log.Println("Bookmark worker started. Listening for events...")

	sig := make(chan os.Signal, 1)
	signal.Notify(sig, os.Interrupt)
	<-sig
	log.Println("Shutting down worker...")
}

func generateMarkdownMock() error {
	panic("unimplemented")
}

// Pass context.Context and jetstream.JetStream (v2) instead of nats.JetStreamContext (v1)
func publishCompletion(ctx context.Context, js jetstream.JetStream, id, storageURL string) {
	event := CompletedEvent{
		ID:         id,
		StorageURL: storageURL,
	}
	payload, _ := json.Marshal(event)

	// v2 Publish requires a context
	if _, err := js.Publish(ctx, "woodhouse.bookmark.completed", payload); err != nil {
		log.Printf("Failed to publish completion event for %s: %v", id, err)
	}
}
