# PLUGIN.RAVENNA delivery

← [Publishing](INDEX.md) · [Repo README](../../README.md)

## Built and tested

The crate `apkaudio-ravenna` (`SRC/`) is a member of the plugin workspace
`PODS/POD.THICK/DISCOVERY.Engine/SRC`: `cargo test -p apkaudio-ravenna` there. The binary `apk-ravenna` is
compiled with every other plugin into the shared image `apk-plugins:local` by
[`DISCOVERY.Engine/Docker/Dockerfile`](../../../../DISCOVERY.Engine/Docker/Dockerfile) (target
`plugins`; the build context is the APK.audio repo root). This repo has no Dockerfile.

## Shipped

As the container `Plugin-RAVENNA` from [`Docker/docker-compose.yml`](../../Docker/docker-compose.yml):
service `ravenna`, profile `discovery`, host network (mDNS does not cross a bridge), `extends: plugin-host` in
[`plugin-base.yml`](../../../../DISCOVERY.Engine/Docker/plugin-base.yml) (image, `cap_drop: [ALL]`,
read-only root, broker variables, log and OUI-cache volumes, the runner's healthcheck).
`DISCOVERY.Engine/Docker/docker-compose.yml` `include:`s this file into the compose project
`apk-audio`, so it starts with the other plugins on a node whose role list names `discovery`.

It lands on the node's broker, under `APK.audio/System/Protocols/ravenna/…` (unretained, restated every 30 s) and `APK.audio/Discovery/ravenna/…` (presence, retained), and in the dashboards as the `ravenna` console family. There is no live URL. The repository
(https://github.com/LikeDotAudio/APK.plugin.discovery.RAVENNA.git) is pushed with every other submodule by `.apk.scripts/deploy_all.py` at the APK.audio
root.

## Not automated

* No CI builds or tests it; the tests run by hand in the workspace.
* No release artefact or version: the image is built locally on each node (`up -d --build`).
* `PLUGIN.DANTE` and `PLUGIN.SAP` compile this crate as a library (its `sdp` module) by path; a change to `src/sdp.rs` reaches them only when the plugin image is rebuilt.
