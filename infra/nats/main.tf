terraform {
  required_providers {
    jetstream = {
      source  = "nats-io/jetstream"
      version = "~> 0.4.0"
    }
  }
}

provider "jetstream" {
  servers = "nats://127.0.0.1:4222"
}

resource "jetstream_stream" "bookmarks" {
  name      = "BOOKMARKS"
  subjects  = ["bookmark.pending", "bookmark.completed"]
  retention = "workqueue"
  storage   = "file"
  discard   = "old"
  
  max_msgs  = 1000
  max_bytes = 1073741824 # 1GB
}

resource "jetstream_stream" "bookmarks_dlq" {
  name      = "BOOKMARKS_DLQ"
  subjects  = ["bookmark.dlq"]
  retention = "limits"
  storage   = "file"
  discard   = "old"
  
  max_age   = 604800 # 7 days
}

# does not exist yet in v0.4! awesome
# resource "jetstream_obj_bucket" "bookmarks_html" {
#   name        = "BOOKMARKS_HTML"
#   description = "Stores raw HTML payloads for bookmarks"
#
#   # Optional: Automatically delete HTML after 7 days if you 
#   # only need it during the initial generation phase. 
#   # Remove this if you want to keep the HTML forever.
#   max_age = 604800 
# }

resource "null_resource" "nats_raw_html_bucket" {
  provisioner "local-exec" {
    # This runs the native NATS CLI command when you run `tofu apply`
    command = "nats obj info raw_html || nats obj add raw_html --description 'Raw HTML payloads'"
  }
}
