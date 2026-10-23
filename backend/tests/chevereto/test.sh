#!/usr/bin/env bash
# End to end test of the Chevereto import: installs a real Chevereto, uploads a few
# images, stores their links in a database mirroring Arcadia's schema, imports
# everything into Elysia and checks files, uploaders and rewritten links.
# Needs docker, cargo and jq. Usage: backend/tests/chevereto/test.sh
set -euo pipefail
cd "$(dirname "$0")"

CHEVERETO=localhost:28083
ELYSIA=http://elysia.test
dc() { docker compose -p elysia-chevereto-test "$@"; }
sql() { local db=$1; shift; dc exec -T db psql -U elysia -d "$db" -v ON_ERROR_STOP=1 -qtA "$@"; }
mariadb() { dc exec -T chevereto_database mariadb -uchevereto -ppassword -N chevereto "$@"; }
fail() { echo "FAIL: $*" >&2; exit 1; }
expect() { [ "$2" = "$3" ] || fail "$1: expected '$3', got '$2'"; }

TEMP_DIR=$(mktemp -d)
trap 'dc down -v; rm -rf "$TEMP_DIR"' EXIT

dc up -d --wait
dc exec -T -u www-data chevereto php app/bin/legacy -C install \
    -u admin -e admin@example.com -x 'Password1234!'

# Three distinct images made with GD and uploaded as guest through API v1.
# Prints each image's file, thumbnail and viewer links, one per line.
links=$(dc exec -T -u www-data chevereto sh -s <<'EOF'
set -e
key=$(php app/bin/legacy -C setting-get -k api_v1_key | tail -n1)
for image in red.png green.jpg blue.gif; do
    php -r '
        [$name, $ext] = explode(".", $argv[1]);
        $colors = ["red" => [200, 30, 30], "green" => [30, 200, 30], "blue" => [30, 30, 200]];
        $gd = imagecreatetruecolor(320, 240);
        imagefill($gd, 0, 0, imagecolorallocate($gd, ...$colors[$name]));
        ["png" => "imagepng", "jpg" => "imagejpeg", "gif" => "imagegif"][$ext]($gd, "/tmp/" . $argv[1]);
    ' "$image"
    curl -sf -F key="$key" -F source=@/tmp/$image http://localhost/api/1/upload |
        php -r '$i = json_decode(stream_get_contents(STDIN), true)["image"];
                echo $i["url"], "\n", $i["thumb"]["url"], "\n", $i["url_viewer"], "\n";'
done
EOF
)
expect "chevereto links" "$(grep -c "^http://$CHEVERETO/" <<<"$links")" 9

# red.png belongs to admin, who has an account in the backend.
mariadb -e "update chv_images set image_user_id = 1 where image_name = 'red'"

# The relevant part of Arcadia's schema, with the links stored the way Arcadia
# does: avatar, BBCode descriptions and cover arrays. link[0..2] are red.png's
# file, thumbnail and viewer links, link[3..5] green.jpg's, link[6..8] blue.gif's.
mapfile -t link <<<"$links"
sql elysia -c "create database backend"
sql backend <<EOF
create table users (
    id serial primary key,
    username varchar(15) unique not null,
    avatar text,
    email varchar(255) unique not null,
    description text not null default ''
);
insert into users (username, avatar, email, description)
values ('admin', '${link[1]}', 'admin@example.com', '[img]${link[0]}[/img] ${link[2]}');
create table title_groups (
    id serial primary key,
    name text not null,
    description text not null,
    covers text [] not null
);
insert into title_groups (name, description, covers)
values ('Album', '[url=${link[5]}][img]${link[4]}[/img][/url] [url=${link[8]}][img]${link[7]}[/img][/url]',
        array['${link[3]}', '${link[6]}']);
EOF
backend_texts="select concat_ws(' ', avatar, description) as text from users
    union all select concat_ws(' ', array_to_string(covers, ' '), description) from title_groups"

cat > "$TEMP_DIR/elysia.yml" <<EOF
server: { host: 127.0.0.1, port: 28090, public_url: $ELYSIA }
database: { host: localhost, port: 25432, user: elysia, password: password, name: elysia }
storage: { upload_dir: $TEMP_DIR/uploads }
auth:
  jwt_secret: chevereto-import-test-secret-of-enough-length
  sso:
    name: Backend
    issuer: http://backend.test/oauth
    client_id: elysia
    client_secret: secret
    redirect_uri: $ELYSIA/auth/sso/callback
EOF
cargo build -q
ELYSIA_CONFIG="$TEMP_DIR/elysia.yml" ../../target/debug/elysia > "$TEMP_DIR/elysia.log" 2>&1 &
SERVER_PID=$!
trap 'kill $SERVER_PID 2>/dev/null; dc down -v; rm -rf "$TEMP_DIR"' EXIT
API=http://127.0.0.1:28090/api
for _ in $(seq 60); do curl -sf -o /dev/null "$API/elysia-unauth-settings" && break; sleep 1; done
TOKEN=$(curl -sf "$API/auth/login" -H 'content-type: application/json' \
    -d '{"login":"admin","password":"change-me-123"}' | jq -r .token)
api() { curl -sf -H "authorization: Bearer $TOKEN" -H 'content-type: application/json' "$@"; }

# Starts an import resuming the saved progress and waits for it to end.
import() {
    api "$API/maintenance/chevereto-import" -d "{
        \"chevereto_database_url\": \"mysql://chevereto:password@localhost:23306/chevereto\",
        \"chevereto_public_url\": \"http://$CHEVERETO\",
        \"backend_database_url\": \"postgres://elysia:password@localhost:25432/backend\",
        \"backend_users\": {\"table\": \"users\", \"username_column\": \"username\",
                          \"subject_column\": \"id\", \"email_column\": \"email\"},
        \"fallback_username\": \"chevereto-import\",
        \"batch_size\": 2, \"concurrency\": 2, \"start\": \"resume\"}"
    while progress=$(api "$API/maintenance/chevereto-import/progress") &&
        [ "$(jq .running <<<"$progress")" = true ]; do
        sleep 1
    done
    [ "$(jq -r .error <<<"$progress")" = null ] || fail "import: $(jq -r .error <<<"$progress")"
    [ "$(jq .failed <<<"$progress")" = 0 ] ||
        fail "failed images: $(api "$API/maintenance/chevereto-import/failures?page=1&page_size=50")"
}
import

expect "imported files" "$(sql elysia -c "select count(*) from files where status = 'available'")" 3
expect "uploaders" "$(sql elysia -c "
    select string_agg(f.original_name || ':' || u.provider || ':' || coalesce(u.external_sub, ''), ' ' order by f.original_name)
    from files f join users u on u.id = f.uploaded_by_user_id")" \
    "blue.gif:local: green.jpg:local: red.png:sso:1"
expect "links left" "$(sql backend -c "select count(*) from ($backend_texts) t where text like '%$CHEVERETO%'")" 0
file_ids=$(sql elysia -c "select string_agg(id, '|') from files")
expect "rewritten links" "$(sql backend -c "
    select count(*) from ($backend_texts) t, regexp_matches(text, '$ELYSIA/[it]/([A-Za-z0-9]+)', 'g') m
    where m[1] ~ '^($file_ids)\$'")" 9

# A rerun resumes after the saved progress and imports nothing new.
import
expect "files after rerun" "$(sql elysia -c "select count(*) from files")" 3

echo "chevereto import test passed"
