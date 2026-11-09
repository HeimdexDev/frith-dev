import io
import json
import unittest
from unittest.mock import Mock
from process import process

SOURCE = b'\0\0\0\x18ftypisom-original'
OUTPUT = b'\0\0\0\x18ftypisom-blurred'

class Missing(Exception):
    response = {'Error': {'Code': '404'}}

class BridgeTests(unittest.TestCase):
    def setup_clients(self):
        s3, sqs = Mock(), Mock()
        s3.put_object.return_value = {'VersionId': 'raw-version'}
        s3.head_object.return_value = {'VersionId': 'approved-output', 'ContentLength': len(OUTPUT)}
        s3.get_object.return_value = {'Body': io.BytesIO(OUTPUT)}
        return s3, sqs

    def test_unique_job_and_pinned_output_without_consuming_results(self):
        s3, sqs = self.setup_clients()
        self.assertEqual(process(s3, sqs, SOURCE, 'raw', 'safe', 'jobs', 'demo/'), OUTPUT)
        job = json.loads(sqs.send_message.call_args.kwargs['MessageBody'])
        self.assertEqual(job['raw']['version_id'], 'raw-version')
        self.assertEqual(job['params']['min_face_px'], 0)
        self.assertTrue(job['output']['key_prefix'].endswith('jobs/' + job['job_id'] + '/'))
        self.assertTrue(job['output']['key_prefix'].startswith('demo/'))
        self.assertEqual(s3.get_object.call_args.kwargs['VersionId'], 'approved-output')
        self.assertEqual([c[0] for c in sqs.mock_calls], ['send_message'])

    def test_missing_output_waits_and_times_out(self):
        s3, sqs = self.setup_clients()
        s3.head_object.side_effect = Missing()
        clock = Mock(side_effect=[0, 0, 301])
        with self.assertRaises(TimeoutError):
            process(s3, sqs, SOURCE, 'raw', 'safe', 'jobs', clock=clock, sleep=lambda _: None)
        s3.get_object.assert_not_called()

    def test_transport_error_is_not_treated_as_pending(self):
        s3, sqs = self.setup_clients()
        s3.head_object.side_effect = PermissionError()
        with self.assertRaises(PermissionError):
            process(s3, sqs, SOURCE, 'raw', 'safe', 'jobs')

    def test_missing_key_without_list_permission_can_return_403(self):
        s3, sqs = self.setup_clients()
        error = Missing()
        error.response = {'Error': {'Code': '403'}}
        s3.head_object.side_effect = [error, s3.head_object.return_value]
        self.assertEqual(process(s3, sqs, SOURCE, 'raw', 'safe', 'jobs', sleep=lambda _: None), OUTPUT)

    def test_unversioned_upload_does_not_enqueue(self):
        s3, sqs = self.setup_clients()
        s3.put_object.return_value = {'VersionId': 'null'}
        with self.assertRaises(ValueError):
            process(s3, sqs, SOURCE, 'raw', 'safe', 'jobs')
        sqs.send_message.assert_not_called()

    def test_invalid_outputs_are_rejected(self):
        for data in (SOURCE, b'not-a-video', b''):
            with self.subTest(data=data):
                s3, sqs = self.setup_clients()
                s3.head_object.return_value['ContentLength'] = len(data)
                s3.get_object.return_value = {'Body': io.BytesIO(data)}
                with self.assertRaises(ValueError):
                    process(s3, sqs, SOURCE, 'raw', 'safe', 'jobs')

if __name__ == '__main__':
    unittest.main()
