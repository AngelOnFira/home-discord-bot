job "home-discord-bot" {
  datacenters = ["dc1"]
  type = "service"

  group "bot" {
    count = 1

    task "discord-bot" {
      driver = "docker"

      config {
        image = "ghcr.io/angelonfira/home-discord-bot:latest"
        force_pull = true
        logging {
          type = "journald"
          config {
            tag = "home-discord-bot"
          }
        }
      }

      template {
        data = <<EOH
DISCORD_TOKEN={{ with nomadVar "nomad/jobs/home-discord-bot" }}{{ .discord_token }}{{ end }}
KASA_USERNAME={{ with nomadVar "nomad/jobs/home-discord-bot" }}{{ .kasa_username }}{{ end }}
KASA_PASSWORD={{ with nomadVar "nomad/jobs/home-discord-bot" }}{{ .kasa_password }}{{ end }}
KASA_DEVICE_IP={{ with nomadVar "nomad/jobs/home-discord-bot" }}{{ .kasa_device_ip }}{{ end }}
KASA_DIR=/opt/python-kasa
EOH
        destination = "local/file.env"
        env = true
      }

      resources {
        cpu    = 200
        memory = 256
      }

      restart {
        attempts = 5
        interval = "5m"
        delay    = "25s"
        mode     = "delay"
      }
    }
  }
} 