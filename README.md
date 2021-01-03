# news.ycombinator.com as a file system*
(* as a 9p file server)

# Usage

```
mkdir -p ~/mnt/hn/
hackernewsfs 'tcp!127.0.0.1!8000'
mount -t 9p -o port=8000 127.0.0.1 ~/mnt/hn/
cd ~/mnt/hn/ && ls
```
