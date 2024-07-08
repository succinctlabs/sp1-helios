import boto3

def generate_presigned_url(bucket_name, object_name, expiration=3600):
    s3_client = boto3.client('s3')
    
    response = s3_client.generate_presigned_url('put_object',
                                                Params={'Bucket': bucket_name, 'Key': object_name},
                                                ExpiresIn=expiration,
                                                HttpMethod='PUT')
    return response

# Example usage
bucket = 'public-circuits'
file_name = 'prover2'
url = generate_presigned_url(bucket, file_name)
print(url)