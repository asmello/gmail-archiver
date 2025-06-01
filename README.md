# Gmail Archiver

WIP

## TODO

- Make `MessagePart` and `MessagePartBody` their own tables in the database, so that
  we can associate `attachmentId` with attachment entries in a relational manner.
- Download attachments and store in database
- Download raw messages and store in database
- Validate parity
- Delete originals

- Implement good rate limiting
- Restore concurrent downloads

## Future

- Search and visualization tool for messages
