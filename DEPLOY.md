## Backend deployment

Fly.io is a useful way to deploy the Bible Web Framework backend. First create a fork, either a public one, or a [private one](https://gist.github.com/0xjac/85097472043b697ab57ba1b1c7530274). The bibles directory from this repository will automatically be pulled in. Now, go to [fly.io](https://fly.io) and deploy the fork you created. It is worth noting that fly.io deployment is *not free*, however, it is fairly affordable, and a simple server that just hosts a few bibles under light to medium load is only like $3 a month.

### Short URLs

With the default fly.toml, the database is specified as immutable, and custom short URL generation is disabled. The database in the `backend/deploy/` directory is baked into the Docker image used on fly.io. If you wish to manually add custom short URLs under this default setup, spin up a server locally with `DATABASE_URL` set to `sqlite://deploy/bwf-deploy.db`, and access the `/v1/short/create` route. Then, once you've stopped the local server, run `sqlite3 deploy/bwf-deploy.db .schema` to clean up any left-over WAL files.
