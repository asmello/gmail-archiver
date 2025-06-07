# Gmail Archiver

There is [Google Takeout], but I wanted to make sure I was getting all the data, and in a structured format.
Having my emails downloaded in two different formats increased the chances that I wasn't going to lose something.

Plus this was a good exercise in API integration.

Use at your own risk.

## Missing features

- Check mode: validate that all messages have been stored
- Delete originals (preferably after validation)
- Good rate limiting (currently just retries with a backoff)
- Concurrent downloads (yes, I'm wasting async)

## Future ideas

### Search and visualization tool

Having all this data is useless if we can't interact with it, right?

### Import mbox

It probably makes sense to add a feature to import an archive in the mbox format,
which is what Google Takeout gives you.

[Google Takeout]: https://takeout.google.com
