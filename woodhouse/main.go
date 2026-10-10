package main

import (
	"context"
	"encoding/json"
	"log"
	"os"
	"os/signal"
	"time"

	md "github.com/JohannesKaufmann/html-to-markdown"
	"github.com/chromedp/chromedp"
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

	// 1. Execute Domain Logic
	md, err := w.generateMarkdown(job.ID)
	if err != nil {
		w.handleFailure(msg, job, meta.NumDelivered, isFinalAttempt)
		return
	}

	// 2. Save to Storage
	if err := w.saveToStorage(job.ID, md); err != nil {
		log.Printf("Failed to save to Object Store: %v", err)
		msg.NakWithDelay(10 * time.Second)
		return
	}

	// 3. Publish Completion & Ack
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
	_, err := w.obs.PutBytes(context.Background(), id+".md", []byte(content))
	return err
}

func (w *Worker) publishCompletion(id, storageURL string) {
	event := CompletedEvent{ID: id, StorageURL: storageURL}
	payload, _ := json.Marshal(event)
	w.js.Publish(context.Background(), "woodhouse.bookmark.completed", payload)
}

func (w *Worker) generateMarkdown(targetURL string) ([]byte, error) {
	// 1. Setup custom allocator options for containerized environments
	opts := append(chromedp.DefaultExecAllocatorOptions[:],
		chromedp.Flag("disable-dev-shm-usage", true), // Fixes the 64MB container crash
	)

	// If the CHROME_BIN environment variable is set (via Docker), tell chromedp to use it
	if chromeBin := os.Getenv("CHROME_BIN"); chromeBin != "" {
		opts = append(opts, chromedp.ExecPath(chromeBin))
	}

	// 2. Create the allocator context
	allocCtx, cancel := chromedp.NewExecAllocator(context.Background(), opts...)
	defer cancel()

	// 3. Create the actual browser context with a timeout
	ctx, cancel := chromedp.NewContext(allocCtx)
	defer cancel()

	ctx, cancel = context.WithTimeout(ctx, 45*time.Second)
	defer cancel()

	var htmlBody string

	// Navigate and grab the HTML
	err := chromedp.Run(ctx,
		chromedp.Navigate(targetURL),
		chromedp.WaitVisible(`body`, chromedp.ByQuery),
		chromedp.OuterHTML(`body`, &htmlBody, chromedp.ByQuery),
	)
	if err != nil {
		return "", err
	}

	converter := md.NewConverter("", true, nil)
	markdown, err := converter.ConvertString(htmlBody)
	if err != nil {
		return "", err
	}

	return []byte(markdown), nil
}

// ---------------------------------------------------------
// INFRASTRUCTURE WIRING (Single Responsibility: Setup)
// ---------------------------------------------------------

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
