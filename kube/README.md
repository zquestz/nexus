# Kubernetes manifests for `nexus-trackerd`

Sample manifests for running the Nexus tracker daemon on a Kubernetes
cluster. Originally written for GKE; adapt the cloud-specific pieces if
you're running elsewhere (see [Customizing](#customizing) below).

## Files

| File                            | Resource     | Purpose                                                                      |
| ------------------------------- | ------------ | ---------------------------------------------------------------------------- |
| `nexus-trackerd-deployment.yml` | `Deployment` | Single-replica daemon with persistent storage for TLS cert + password hashes |
| `nexus-trackerd-srv.yml`        | `Service`    | LoadBalancer fronting the TCP (7510) and WebSocket (7511) ports              |

## Quick start (GKE)

1. Create a persistent disk in the same zone as your cluster. GCE
   enforces a 10 GiB minimum even though the tracker needs only a
   handful of MiB:

   ```sh
   gcloud compute disks create nexus-tracker-data \
     --size=10GiB --type=pd-standard --zone=<your-zone>
   ```

2. Reserve a regional static IP (or remove `loadBalancerIP` to let GCP
   pick one):

   ```sh
   gcloud compute addresses create nexus-tracker-ip \
     --region=<your-region>
   ```

3. Edit `nexus-trackerd-srv.yml` and replace `loadBalancerIP` with your
   reserved address.

4. Apply:

   ```sh
   kubectl apply -f kube/nexus-trackerd-deployment.yml
   kubectl apply -f kube/nexus-trackerd-srv.yml
   ```

> **Disk ownership.** A freshly-attached GCE PD is formatted ext4
> with the mount root owned by `root:root` and mode `0o755`, but the
> daemon runs as uid 1000 and locks its data directory to `0o700`.
> The deployment includes `securityContext.fsGroup: 1000` so kubelet
> chowns the volume to gid 1000 on mount; the daemon (running as
> that gid) can then chmod within. If you swap to a different volume
> backend, preserve the `fsGroup` setting or use an init container
> that runs `chown -R 1000:1000` + `chmod 700` before the daemon
> starts.

## Customizing

Most installs need to touch at least these fields:

- **`metadata.namespace`** — defaults to `default`; change in both files.
- **`spec.template.spec.containers[0].image`** — defaults to
  `ghcr.io/zquestz/nexus-trackerd:latest`. Pin a specific version
  (`:0.1.1`) for reproducible deploys.
- **`loadBalancerIP`** in the Service — set to your reserved address or
  delete the line.
- **Volume backend** — the deployment uses `gcePersistentDisk` (GKE
  only). On EKS, AKS, on-prem, etc., replace the `volumes:` block with
  a `PersistentVolumeClaim` referencing a `StorageClass` your cluster
  provides.
- **`cloud.google.com/network-tier: Standard`** annotation in the
  Service — GCP-specific. Remove on non-GKE clusters.

## Configuration

The image reads its configuration from environment variables. The
deployment sets only `NEXUS_TRACKER_WEBSOCKET=1` (to enable port 7511);
everything else uses the defaults baked into the Dockerfile. Add more
env vars to the `containers[0].env` list to tune limits, log level,
rate caps, etc.

Full env var reference: [`docs/tracker/03-docker.md`](../docs/tracker/03-docker.md).

## Why these choices

- **`replicas: 1` + `strategy: Recreate`** — the persistent disk is
  ReadWriteOnce, and the in-memory registry is single-instance by
  design. Don't scale horizontally.
- **`externalTrafficPolicy: Local`** — preserves the client source IP,
  which the tracker uses for per-IP rate limiting and the
  `max-entries-per-ip` cap. Without this, every connection looks like
  it came from a node IP and the limits collapse.
- **Mount path `/home/nexus-tracker/.local/share/nexus-trackerd/`** —
  matches the daemon's platform default data directory inside the
  container. The TLS cert + key, password hashes, and (by default) log
  files live here. Losing this volume means clients have to re-pin the
  fingerprint after the next start.

## Operator docs

- Daemon configuration: [`docs/tracker/02-configuration.md`](../docs/tracker/02-configuration.md)
- Password management (registration / listing): [`docs/tracker/04-passwords.md`](../docs/tracker/04-passwords.md)
- Troubleshooting: [`docs/tracker/05-troubleshooting.md`](../docs/tracker/05-troubleshooting.md)
