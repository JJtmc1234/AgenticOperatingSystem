import contextlib
import io
import tempfile
import unittest
from unittest.mock import patch
from iris_workflow.__main__ import main

class StatusCliTests(unittest.TestCase):
    def test_status_does_not_write_to_the_state_directory(self):
        with tempfile.TemporaryDirectory() as directory, \
             patch('sys.argv',['aos-iris','--home',directory,'status']), \
             patch('iris_workflow.status.write',side_effect=PermissionError('read-only')), \
             contextlib.redirect_stdout(io.StringIO()) as output:
            self.assertEqual(main(),0)
            self.assertIn('Published issues',output.getvalue())
