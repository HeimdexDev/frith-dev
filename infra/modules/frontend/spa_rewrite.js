// CloudFront Function (cloudfront-js-2.0), viewer-request, attached to the WEB (S3) behavior only.
// Client-side routes ("/datasets/42") have no file extension: serve the SPA shell.
// Files ("/assets/app.js") pass through, so a missing asset stays a real 403/404, and
// /api/* never reaches this function because it has its own cache behavior.
function handler(event) {
  var request = event.request;
  var uri = request.uri;
  var last = uri.substring(uri.lastIndexOf("/") + 1);
  if (last.indexOf(".") === -1) {
    request.uri = "/index.html";
  }
  return request;
}
