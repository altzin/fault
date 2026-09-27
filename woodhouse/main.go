// package main
//
// import (
//
//	"encoding/json"
//	"log"
//	"os"
//	"os/signal"
//	"time"
//
//	"github.com/nats-io/nats.go"
//
// )
//
//	type BookmarkJob struct {
//		ID  string `json:"id"`
//		URL string `json:"url"`
//	}
//
//	type CompletedEvent struct {
//		ID         string `json:"id"`
//		StorageURL string `json:"storage_url"`
//	}
//
//	func main() {
//		nc, err := nats.Connect(nats.DefaultURL) // Update with your NATS URL
//		if err != nil {
//			log.Fatalf("failed to connect to nats: %v", err)
//		}
//		defer nc.Close()
//
//		js, err := nc.JetStream()
//		if err != nil {
//			log.Fatalf("failed to get jetstream context: %v", err)
//		}
//
//		storage := NewStorageBackend()
//
//		// Durable queue subscription ensures load balancing across multiple worker instances
//		sub, err := js.QueueSubscribe("bookmark.pending", "bookmark-workers", func(msg *nats.Msg) {
//			meta, _ := msg.Metadata()
//
//			// Poison Pill / DLQ handling
//			if meta.NumDelivered > 3 {
//				log.Printf("Message failed 3 times, routing to DLQ: %s", string(msg.Data))
//				js.Publish("bookmark.dlq", msg.Data)
//				msg.Term() // Permanently clear from queue
//				return
//			}
//
//			var job BookmarkJob
//			if err := json.Unmarshal(msg.Data, &job); err != nil {
//				log.Printf("Invalid payload, terminating message: %v", err)
//				msg.Term()
//				return
//			}
//
//			// 1. Idempotency Check
//			objectKey := GenerateStorageKey(job.URL)
//			if storage.Exists(objectKey) {
//				log.Printf("Idempotency hit: %s already exists. Skipping generation.", objectKey)
//				publishCompletion(js, job.ID, objectKey)
//				msg.Ack()
//				return
//			}
//
//			// 2. Generate Markdown
//			mdBytes, err := GenerateMarkdown(job.URL)
//			if err != nil {
//				log.Printf("Failed to generate markdown for %s: %v", job.URL, err)
//				msg.NakWithDelay(30 * time.Second) // Requeue to try again later
//				return
//			}
//
//			// 3. Upload to Storage (S3 / GarageHQ)
//			if err := storage.Upload(objectKey, mdBytes); err != nil {
//				log.Printf("Failed to upload to storage: %v", err)
//				msg.Nak()
//				return
//			}
//
//			// 4. Publish completion event back to Axum
//			publishCompletion(js, job.ID, objectKey)
//
//			// 5. Ack original message to clear it from the pending queue
//			msg.Ack()
//
//		}, nats.ManualAck()) // Explicit Ack required
//
//		if err != nil {
//			log.Fatalf("failed to subscribe: %v", err)
//		}
//		defer sub.Unsubscribe()
//
//		log.Println("Bookmark worker started. Listening for events...")
//
//		// Graceful shutdown
//		sig := make(chan os.Signal, 1)
//		signal.Notify(sig, os.Interrupt)
//		<-sig
//		log.Println("Shutting down worker...")
//	}
//
//	func publishCompletion(js nats.JetStreamContext, id, storageURL string) {
//		event := CompletedEvent{
//			ID:         id,
//			StorageURL: storageURL,
//		}
//		payload, _ := json.Marshal(event)
//		if _, err := js.Publish("bookmark.completed", payload); err != nil {
//			log.Printf("Failed to publish completion event for %s: %v", id, err)
//			// We log but do not fail the function; if this fails, the NATS AckWait timeout
//			// will trigger a redelivery, and the idempotency check will catch it next time.
//		}
//	}
package main

import (
	"encoding/json"
	"log"
	"os"
	"os/signal"
	"time"

	"github.com/nats-io/nats.go"
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

	js, err := nc.JetStream()
	if err != nil {
		log.Fatalf("failed to get jetstream context: %v", err)
	}

	// The nats.MaxDeliver(3) option configures the consumer natively
	sub, err := js.QueueSubscribe("bookmark.pending", "bookmark-workers", func(msg *nats.Msg) {
		var job BookmarkJob
		if err := json.Unmarshal(msg.Data, &job); err != nil {
			log.Printf("Invalid payload, terminating message: %v", err)
			msg.Term() // Natively tells NATS to never redeliver this message
			return
		}

		mdBytes, err := GenerateMarkdown(job.URL)
		if err != nil {
			log.Printf("Failed to generate markdown for %s: %v", job.URL, err)
			// NAK tells NATS to redeliver. NATS tracks the count natively.
			// After 3 attempts, NATS will drop it.
			msg.NakWithDelay(30 * time.Second)
			return
		}

		log.Printf("SUCCESS: Generated %d bytes of markdown for %s", len(mdBytes), job.URL)

		publishCompletion(js, job.ID, "local://stdout-only")
		msg.Ack()

	}, nats.ManualAck(), nats.MaxDeliver(3))

	if err != nil {
		log.Fatalf("failed to subscribe: %v", err)
	}
	defer sub.Unsubscribe()

	log.Println("Bookmark worker started. Listening for events...")

	sig := make(chan os.Signal, 1)
	signal.Notify(sig, os.Interrupt)
	<-sig
	log.Println("Shutting down worker...")
}

func publishCompletion(js nats.JetStreamContext, id, storageURL string) {
	event := CompletedEvent{
		ID:         id,
		StorageURL: storageURL,
	}
	payload, _ := json.Marshal(event)
	if _, err := js.Publish("bookmark.completed", payload); err != nil {
		log.Printf("Failed to publish completion event for %s: %v", id, err)
	}
}
