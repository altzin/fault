package main

import (
	"context"
	"encoding/json"
	"log"
	"os"
	"os/signal"
	"time"

	md "github.com/JohannesKaufmann/html-to-markdown"
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

type Worker struct {
	js  jetstream.JetStream
	obs jetstream.ObjectStore
}

func (w *Worker) HandleMessage(msg jetstream.Msg) {
	meta, _ := msg.Metadata()
	isFinalAttempt := meta.NumDelivered >= 3

	var job BookmarkJob
	if err := json.Unmarshal(msg.Data(), &job); err != nil {
		log.Printf("Invalid payload: %v", err)
		msg.Term() // Bad JSON will never succeed, terminate it
		return
	}

	// 1. Fetch the raw HTML that Cheryl saved to the Object Store
	htmlBytes, err := w.obs.GetBytes(context.Background(), job.ID+".html")
	if err != nil {
		log.Printf("Failed to fetch HTML for %s: %v", job.ID, err)
		w.handleFailure(msg, job, meta.NumDelivered, isFinalAttempt)
		return
	}

	// 2. Execute Domain Logic (Pure string manipulation, no network!)
	mdBytes, err := w.generateMarkdown(htmlBytes)
	if err != nil {
		log.Printf("Failed to convert HTML to Markdown: %v", err)
		w.handleFailure(msg, job, meta.NumDelivered, isFinalAttempt)
		return
	}

	// 3. Save Markdown to Storage
	if err := w.saveToStorage(job.ID, mdBytes); err != nil {
		log.Printf("Failed to save to Object Store: %v", err)
		msg.NakWithDelay(10 * time.Second)
		return
	}

	// 4. Publish Completion & Ack
	storageURL := "nats://raw_html/" + job.ID + ".md"
	w.publishCompletion(job.ID, storageURL)
	msg.Ack()
	log.Printf("SUCCESS: Processed bookmark %s", job.ID)
}

func (w *Worker) handleFailure(msg jetstream.Msg, job BookmarkJob, attempts uint64, isFinal bool) {
	if isFinal {
		log.Printf("[DLQ] Max deliveries reached for %s. Routing to DLQ.", job.URL)
		w.js.Publish(context.Background(), "woodhouse.bookmark.dlq", msg.Data())
		msg.Ack()
		return
	}
	log.Printf("Attempt %d failed for %s", attempts, job.URL)
	msg.NakWithDelay(30 * time.Second)
}

func (w *Worker) saveToStorage(id string, content []byte) error {
	log.Printf("Saved %v:\n %v", id, string(content))
	_, err := w.obs.PutBytes(context.Background(), id+".md", []byte(content))
	return err
}

func (w *Worker) publishCompletion(id, storageURL string) {
	event := CompletedEvent{ID: id, StorageURL: storageURL}
	payload, _ := json.Marshal(event)
	w.js.Publish(context.Background(), "woodhouse.bookmark.completed", payload)
}

func (w *Worker) generateMarkdown(htmlBytes []byte) ([]byte, error) {
	// No context, no timeouts, no headless browsers.
	converter := md.NewConverter("", true, nil)

	markdown, err := converter.ConvertBytes(htmlBytes)
	if err != nil {
		return nil, err
	}

	return markdown, nil
}

func main() {
	natsURL := os.Getenv("NATS_URL")
	nc, err := nats.Connect(natsURL)
	if err != nil {
		log.Fatalf("Failed to connect to NATS: %v", err)
	}
	defer nc.Close()

	js, _ := jetstream.New(nc)
	obs, _ := js.ObjectStore(context.Background(), "raw_html")

	// Inject dependencies into the Worker
	worker := &Worker{
		js:  js,
		obs: obs,
	}

	// Setup Stream and Consumer... (omitted error handling for brevity)
	stream, _ := js.Stream(context.Background(), "WOODHOUSE_EVENTS")
	cons, _ := stream.CreateOrUpdateConsumer(context.Background(), jetstream.ConsumerConfig{
		Durable:       "bookmark-workers",
		DeliverGroup:  "bookmark-workers",
		FilterSubject: "woodhouse.bookmark.pending",
		MaxDeliver:    3,
		AckPolicy:     jetstream.AckExplicitPolicy,
	})

	// Start consuming using the Worker's method as the callback
	cc, _ := cons.Consume(worker.HandleMessage)
	defer cc.Stop()

	log.Println("Bookmark worker started. Listening for events...")
	sig := make(chan os.Signal, 1)
	signal.Notify(sig, os.Interrupt)
	<-sig
	log.Println("Shutting down worker...")
}
